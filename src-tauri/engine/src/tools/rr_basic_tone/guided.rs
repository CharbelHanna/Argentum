//! RapidRAW's guided filter on log luminance: what tells the tone sliders a
//! pixel's local detail from the light it sits in.
//!
//! The filter fits, around every pixel, a line from the log luminance to a
//! smoothed log luminance (He, Sun and Tang's guided filter, with the picture
//! guiding itself), on a copy at most 1080 pixels on its short side. Its two
//! coefficients per pixel are kept at that size and read back bilinearly.
//! Two scales are kept: a fine one (radius 10 at 1080) and a coarse one
//! (radius 40, on a quarter-size copy). In log luminance a change of exposure
//! moves every coefficient `b` by the same amount and leaves the detail where
//! it was.
//!
//! Copied from the app @ c786c146: the passes from
//! `src-tauri/src/shaders/guided.wgsl` (`gf_downsample`, `gf_box_down`,
//! `gf_blur_h`, `gf_blur_v`, `gf_coeffs`), their order and sizes from
//! `build_guided_coeffs` in `src-tauri/src/gpu_processing.rs`, and the
//! reading back from `sample_gf_tex` in `src-tauri/src/shaders/shader.wgsl`.
//! RapidRAW's "log-space guided filter for edge-aware adjustments"
//! (66fa1600). Only the luminance half: the darkest-channel half is for
//! dehaze, and the clarity scale for clarity, neither part of this tool.

use rayon::prelude::*;

use crate::Rgb;
use crate::colour::luma;

const GF_SHORT_EDGE_PX: u32 = 1080;
const GF_COARSE_DIV: u32 = 4;
const GF_STRUCTURE_RADIUS_PX: f32 = 40.0;
const GF_STRUCTURE_EPS: f32 = 0.5;
const GF_TONAL_RADIUS_PX: f32 = 10.0;
const GF_TONAL_EPS: f32 = 0.25;
/// The darkest luminance the logarithm is taken of.
pub const GF_LUMA_FLOOR: f32 = 1.0e-4;
/// The highest value the filter reads: half float's largest, as the app's
/// input texture holds.
const HALF_MAX: f32 = 65504.0;

/// A small picture of two numbers per pixel: the moments of log luminance,
/// or the fitted line's slope and offset.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub texels: Vec<[f32; 2]>,
}

/// Both scales' coefficients for one picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Coefficients {
    /// The picture they were measured on, in pixels.
    pub full: (usize, usize),
    pub tonal: Plane,
    pub coarse: Plane,
}

impl Coefficients {
    /// `rgb`'s coefficients, as `build_guided_coeffs` makes them.
    pub fn of(rgb: &Rgb) -> Coefficients {
        let short = rgb.width.min(rgb.height).max(1) as u32;
        let k = (GF_SHORT_EDGE_PX as f32 / short as f32).min(1.0);
        let lw = ((rgb.width as f32 * k).round() as usize).max(1);
        let lh = ((rgb.height as f32 * k).round() as usize).max(1);
        let low_short = lw.min(lh) as f32;
        let cw = lw.div_ceil(GF_COARSE_DIV as usize).max(1);
        let ch = lh.div_ceil(GF_COARSE_DIV as usize).max(1);
        let radius = |base: f32, div: f32| (base * low_short / 1080.0 / div).ceil().max(1.0) as i32;

        let moments = downsample(rgb, lw, lh);

        let coarse_moments = box_down(&moments, cw, ch);
        let coarse = fit(&coarse_moments, radius(GF_STRUCTURE_RADIUS_PX, GF_COARSE_DIV as f32), GF_STRUCTURE_EPS);
        let tonal = fit(&moments, radius(GF_TONAL_RADIUS_PX, 1.0), GF_TONAL_EPS);

        Coefficients {
            full: (rgb.width, rgb.height),
            tonal,
            coarse,
        }
    }

    /// The local and the mid-scale log detail of the pixel at `(x, y)`,
    /// whose log luminance is `log_luma`: `tonal_log_detail` and
    /// `tonal_mid_detail` in the shader's `main`, with no clarity or
    /// structure.
    pub fn detail(&self, x: usize, y: usize, log_luma: f32) -> (f32, f32) {
        let [ta, tb] = sample(&self.tonal, self.full, x, y);
        let [ca, cb] = sample(&self.coarse, self.full, x, y);
        let smooth = ta * log_luma + tb;
        (log_luma - smooth, smooth - (ca * log_luma + cb))
    }
}

/// The log luminance the filter fits and the detail is measured against.
pub fn log_luma(c: [f32; 3]) -> f32 {
    luma(c.map(|v| v.max(0.0))).max(GF_LUMA_FLOOR).log2()
}

/// `gf_downsample`, the luminance half: the mean of log luminance and of its
/// square under each low-resolution pixel, from up to 8 x 8 taps.
fn downsample(rgb: &Rgb, w: usize, h: usize) -> Plane {
    let src = (rgb.width, rgb.height);
    let texels = taps_mean(src, (w, h), |x, y| {
        let p = rgb.pixels[y * rgb.width + x].map(|v| v.clamp(0.0, HALF_MAX));
        let l = luma(p).max(GF_LUMA_FLOOR).log2();
        [l, l * l]
    });
    Plane {
        width: w,
        height: h,
        texels,
    }
}

/// `gf_box_down`: the same taps, averaging the moments themselves.
fn box_down(src: &Plane, w: usize, h: usize) -> Plane {
    let texels = taps_mean((src.width, src.height), (w, h), |x, y| {
        src.texels[y * src.width + x]
    });
    Plane {
        width: w,
        height: h,
        texels,
    }
}

/// The tap pattern both downsamples share: for each output pixel, up to
/// 8 x 8 points spread evenly over the source pixels under it.
fn taps_mean(
    src: (usize, usize),
    dst: (usize, usize),
    read: impl Fn(usize, usize) -> [f32; 2] + Sync,
) -> Vec<[f32; 2]> {
    let ratio = [src.0 as f32 / dst.0 as f32, src.1 as f32 / dst.1 as f32];
    let taps = ratio.map(|r| r.ceil().clamp(1.0, 8.0) as i32);
    let max = [src.0 as i32 - 1, src.1 as i32 - 1];
    (0..dst.0 * dst.1)
        .into_par_iter()
        .map(|i| {
            let origin = [(i % dst.0) as f32 * ratio[0], (i / dst.0) as f32 * ratio[1]];
            let mut sum = [0.0f32; 2];
            for j in 0..taps[1] {
                for t in 0..taps[0] {
                    let px = origin[0] + (t as f32 + 0.5) / taps[0] as f32 * ratio[0];
                    let py = origin[1] + (j as f32 + 0.5) / taps[1] as f32 * ratio[1];
                    let x = (px as i32).clamp(0, max[0]) as usize;
                    let y = (py as i32).clamp(0, max[1]) as usize;
                    let v = read(x, y);
                    sum[0] += v[0];
                    sum[1] += v[1];
                }
            }
            let n = (taps[0] * taps[1]) as f32;
            [sum[0] / n, sum[1] / n]
        })
        .collect()
}

/// Blur the moments, fit the line at each pixel (`gf_coeffs`), and blur the
/// lines: one scale of the filter, as `build_guided_coeffs` runs it.
fn fit(moments: &Plane, radius: i32, eps: f32) -> Plane {
    let blurred = blur(moments, radius);
    let lines = Plane {
        texels: blurred
            .texels
            .par_iter()
            .map(|&[mean, mean_sq]| {
                let variance = (mean_sq - mean * mean).max(0.0);
                let a = variance / (variance + eps);
                [a, mean * (1.0 - a)]
            })
            .collect(),
        ..blurred
    };
    blur(&lines, radius)
}

/// `gf_blur_h` then `gf_blur_v`: a gaussian of sigma `radius / 2`, cut at
/// `radius`, its weights summed as they are applied, edges repeated.
fn blur(src: &Plane, radius: i32) -> Plane {
    let sigma = (radius as f32 * 0.5).max(0.5);
    let weights: Vec<f32> = (-radius..=radius)
        .map(|o| {
            let x = o as f32;
            (-(x * x) / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    let pass = |src: &[[f32; 2]], (w, h): (usize, usize), dir: (i32, i32)| -> Vec<[f32; 2]> {
        (0..w * h)
            .into_par_iter()
            .map(|i| {
                let (x, y) = ((i % w) as i32, (i / w) as i32);
                let mut total = [0.0f32; 2];
                let mut weight_sum = 0.0f32;
                for (k, &wt) in weights.iter().enumerate() {
                    let o = k as i32 - radius;
                    let cx = (x + dir.0 * o).clamp(0, w as i32 - 1) as usize;
                    let cy = (y + dir.1 * o).clamp(0, h as i32 - 1) as usize;
                    let v = src[cy * w + cx];
                    total[0] += v[0] * wt;
                    total[1] += v[1] * wt;
                    weight_sum += wt;
                }
                [total[0] / weight_sum, total[1] / weight_sum]
            })
            .collect()
    };
    let size = (src.width, src.height);
    let across = pass(&src.texels, size, (1, 0));
    Plane {
        width: src.width,
        height: src.height,
        texels: pass(&across, size, (0, 1)),
    }
}

/// `sample_gf_tex`: the plane read bilinearly at the centre of full-size
/// pixel `(x, y)`, edges repeated.
fn sample(plane: &Plane, full: (usize, usize), x: usize, y: usize) -> [f32; 2] {
    let low = (plane.width as i32, plane.height as i32);
    let px = (x as f32 + 0.5) / full.0 as f32 * low.0 as f32 - 0.5;
    let py = (y as f32 + 0.5) / full.1 as f32 * low.1 as f32 - 0.5;
    let (fx, fy) = (px.floor(), py.floor());
    let (wx, wy) = (px - fx, py - fy);
    let at = |cx: i32, cy: i32| {
        let cx = cx.clamp(0, low.0 - 1) as usize;
        let cy = cy.clamp(0, low.1 - 1) as usize;
        plane.texels[cy * plane.width + cx]
    };
    let (x0, y0) = (fx as i32, fy as i32);
    let mix = |a: [f32; 2], b: [f32; 2], t: f32| [a[0] * (1.0 - t) + b[0] * t, a[1] * (1.0 - t) + b[1] * t];
    mix(
        mix(at(x0, y0), at(x0 + 1, y0), wx),
        mix(at(x0, y0 + 1), at(x0 + 1, y0 + 1), wx),
        wy,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize, f: impl Fn(usize, usize) -> f32) -> Rgb {
        let pixels = (0..w * h).map(|i| [f(i % w, i / w); 3]).collect();
        Rgb::new(w, h, pixels)
    }

    #[test]
    fn a_flat_picture_has_no_detail() {
        let c = Coefficients::of(&picture(64, 48, |_, _| 0.18));
        let (local, mid) = c.detail(10, 10, log_luma([0.18; 3]));
        assert!(local.abs() < 1e-5 && mid.abs() < 1e-5, "{local} {mid}");
    }

    #[test]
    fn exposure_leaves_the_detail_where_it_was() {
        let texture = |x: usize, y: usize| 0.05 + 0.1 * (((x * 7 + y * 13) % 11) as f32 / 11.0);
        let a = Coefficients::of(&picture(80, 60, texture));
        let b = Coefficients::of(&picture(80, 60, |x, y| 4.0 * texture(x, y)));
        for (x, y) in [(5, 5), (40, 30), (79, 59)] {
            let v = texture(x, y);
            let da = a.detail(x, y, log_luma([v; 3]));
            let db = b.detail(x, y, log_luma([4.0 * v; 3]));
            assert!((da.0 - db.0).abs() < 1e-4 && (da.1 - db.1).abs() < 1e-4, "{da:?} {db:?}");
        }
    }

    #[test]
    fn the_filter_works_at_most_1080_pixels_on_the_short_side() {
        let c = Coefficients::of(&picture(2400, 1600, |_, _| 0.5));
        assert_eq!((c.tonal.width, c.tonal.height), (1620, 1080));
        assert_eq!((c.coarse.width, c.coarse.height), (405, 270));
    }
}
