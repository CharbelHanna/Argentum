//! RawTherapee's capture sharpening, on the engine's own GPU device.
//!
//! Richardson-Lucy deconvolution of the luminance with a gaussian PSF: it
//! undoes the blur of the lens and the sensor rather than adding edge
//! contrast. RawTherapee `rtengine/capturesharpening.cc`
//! @ c6d04960e14fed89e531cbbdf680021d131b4685, with its contrast mask from
//! `rtengine/rt_algo.cc` @ 1c2b3f33772eaf491b7bd4d66323ac36d13aa344, so flat
//! areas - sky, skin, noise - are left alone.
//!
//! Copied from the app @ 2bcb953d: the settings, the radius read from the
//! RAW and the automatic contrast threshold from `src-tauri/src/mods/sharpen.rs`,
//! the passes from `mods/sharpen_gpu.rs` and `shaders/sharpen.wgsl`. Capture
//! sharpening only - the app's darktable unsharp mask, each photo mask's own
//! Sharpen and the mask view are not part of this tool.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - Its own device (`crate::Gpu`), not RapidRAW's processor.
//! - The pipeline's f32 pixels go up as an rgba32float texture, and what
//!   comes back is the factor each pixel's luminance moved by, which is
//!   applied to the f32 pixels here. The app reads and writes half floats.
//!   Given the same input, the two agree to within one half-float step.
//! - The whole picture every time. The pipeline keeps the result, so the
//!   app's kept textures, regions of interest and panning have no job here.
//! - The automatic contrast threshold is measured on the picture this tool
//!   is given. The app measures on the full-size decode even for a preview;
//!   at a preview's size the engine measures the preview. At fit-to-screen
//!   capture sharpening is below a pixel and skipped either way.
//!
//! RADII ARE IN FULL-RESOLUTION PIXELS
//!
//! The radius means the same thing at every size: it is scaled by the
//! picture's render scale, as darktable scales by `roi_in->scale` and
//! RawTherapee divides by its preview scale.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use rawler::rawimage::{RawImage, RawImageData};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{Context, Error, Gpu, Image, Kind, Range, Result, Rgb, Stage, Tool};

/// RawTherapee's defaults, from `rtgui/tools/pdsharpening.cc`
/// @ c777f374b4db9ec5ecec3dcfea2e857382208ea9: contrast 10 (auto on),
/// radius 0.75 (auto on), corner boost 0, 20 iterations.
pub const DEFAULT_CONTRAST: f32 = 10.0;
pub const DEFAULT_RADIUS: f32 = 0.75;
pub const DEFAULT_ITERATIONS: u32 = 20;

/// The settings, named as the app's `agSharpen` keys for the same things.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// On for every RAW unless turned off.
    pub capture: bool,
    /// 0..100: how much of the deconvolved luminance is blended in.
    pub capture_amount: f32,
    /// Read the radius from the RAW's own sharpest edge.
    pub auto_radius: bool,
    /// Gaussian sigma of the PSF, full-resolution pixels, when not automatic.
    pub radius: f32,
    /// RawTherapee's corner boost: sigma added at the corners.
    pub corner_boost: f32,
    pub iterations: u32,
    /// RawTherapee's check that stops a 32 px block once deconvolution starts
    /// to ring.
    pub iter_check: bool,
    /// Find the contrast threshold from the flattest part of the photo.
    pub auto_contrast: bool,
    /// RawTherapee's contrast threshold, when not automatic. 0 sharpens
    /// everywhere.
    pub contrast: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture: true,
            capture_amount: 100.0,
            auto_radius: true,
            radius: DEFAULT_RADIUS,
            corner_boost: 0.0,
            iterations: DEFAULT_ITERATIONS,
            iter_check: true,
            auto_contrast: true,
            contrast: DEFAULT_CONTRAST,
        }
    }
}

impl crate::Settings for Settings {
    const VERSION: u32 = 1;
    // The app's clamps, `sharpen::from_json`.
    const RANGES: &'static [Range] = &[
        Range::new("captureAmount", 0.0, 100.0),
        Range::new("radius", 0.4, 2.0),
        Range::new("cornerBoost", -0.5, 0.5),
        Range::new("iterations", 1.0, 100.0),
        Range::new("contrast", 0.0, 200.0),
    ];
}

#[derive(Default)]
pub struct RtCaptureSharpen {
    /// The radius the last photo's mosaic gave, by source key. A pass over
    /// the whole mosaic, made once per photo.
    radius: Mutex<Option<(u64, Option<f32>)>>,
    /// The contrast threshold last measured, by input key.
    contrast: Mutex<Option<(u64, f32)>>,
    engine: Mutex<Option<Engine>>,
}

impl Tool for RtCaptureSharpen {
    const ID: &'static str = "rt_capture_sharpen";
    const STAGE: Stage = Stage::SceneLinear;
    const KIND: Kind = Kind::GpuMultiPass;
    type Settings = Settings;

    fn process(&self, settings: &Settings, input: &Image, cx: &Context) -> Result<Option<Image>> {
        let rgb = input.as_rgb().ok_or_else(|| Error::Tool {
            tool: Self::ID,
            message: format!("was handed {}", input.describe()),
        })?;
        if !settings.capture {
            return Ok(None);
        }
        let radius = if settings.auto_radius {
            self.auto_radius(cx.source.key, &cx.source.raw)
                .unwrap_or(DEFAULT_RADIUS)
        } else {
            settings.radius.clamp(0.4, 2.0)
        };
        let resolved = Resolved {
            amount: (settings.capture_amount / 100.0).clamp(0.0, 1.0),
            radius,
            corner: settings.corner_boost.clamp(-0.5, 0.5),
            iterations: settings.iterations.clamp(1, 100),
            iter_check: settings.iter_check,
            contrast: 0.0,
        };
        // Asked before the threshold is measured, since that is a pass over
        // the whole photo: at fit-to-screen there is nothing to do.
        if plan(&resolved, rgb.width, rgb.height, rgb.scale()).is_none() {
            return Ok(None);
        }
        let contrast = if settings.auto_contrast {
            self.auto_contrast(cx.input_key, rgb)
        } else {
            settings.contrast.clamp(0.0, 200.0) / 100.0
        };
        let plan = plan(
            &Resolved {
                contrast,
                ..resolved
            },
            rgb.width,
            rgb.height,
            rgb.scale(),
        )
        .expect("the contrast threshold does not decide whether it runs");
        let gpu = cx.gpu()?;
        let pixels = self.sharpen(&gpu, rgb, &plan)?;
        Ok(Some(Image::Rgb(rgb.with_pixels(pixels))))
    }
}

impl RtCaptureSharpen {
    fn auto_radius(&self, key: u64, raw: &RawImage) -> Option<f32> {
        let mut memo = self.radius.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, r)) = *memo
            && k == key
        {
            return r;
        }
        let r = measure_radius(raw);
        *memo = Some((key, r));
        r
    }

    fn auto_contrast(&self, key: u64, rgb: &Rgb) -> f32 {
        let mut memo = self.contrast.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, c)) = *memo
            && k == key
        {
            return c;
        }
        let c = measure_contrast(rgb);
        *memo = Some((key, c));
        c
    }

    /// Run the passes and apply what they found to `rgb`'s own pixels.
    fn sharpen(&self, gpu: &Arc<Gpu>, rgb: &Rgb, plan: &Plan) -> Result<Vec<[f32; 3]>> {
        let mut slot = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        if slot.as_ref().is_none_or(|e| !Arc::ptr_eq(&e.gpu, gpu)) {
            *slot = Some(Engine::new(gpu));
        }
        let engine = slot.as_ref().expect("just made");
        let ratios = engine.ratios(rgb, plan)?;
        Ok(rgb
            .pixels
            .par_iter()
            .zip(ratios.par_iter())
            .map(|(p, &r)| {
                // Exactly 1 where nothing moved: copied, not multiplied.
                if r == 1.0 {
                    *p
                } else {
                    [p[0] * r, p[1] * r, p[2] * r]
                }
            })
            .collect())
    }
}

/// The settings with everything automatic worked out.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Resolved {
    /// 0..1.
    amount: f32,
    radius: f32,
    corner: f32,
    iterations: u32,
    iter_check: bool,
    /// 0..1. 0 sharpens everywhere.
    contrast: f32,
}

/// Measure the radius a RAW's mosaic gives, as the app does at decode -
/// `None` for sRAW, linear DNGs and anything else without a mosaic.
pub fn measure_radius(raw: &RawImage) -> Option<f32> {
    measure_radius_impl(raw)
}

/// Measure the automatic contrast threshold (0..1) on linear RGB, as the app
/// does on the image its render is given.
pub fn measure_contrast(rgb: &Rgb) -> f32 {
    let l: Vec<f32> = rgb
        .pixels
        .par_iter()
        .map(|p| rt_l(p[0], p[1], p[2]))
        .collect();
    auto_contrast_threshold(&l, rgb.width, rgb.height)
}

// ---------------------------------------------------------------------------
// Auto radius: read from the RAW. Copied from src-tauri/src/mods/sharpen.rs
// @ 2bcb953d.
// ---------------------------------------------------------------------------

/// RawTherapee's ceiling for an automatic radius (`maxSigma`).
const MAX_AUTO_RADIUS: f32 = 2.0;

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d (`measure_radius`).
/// RawTherapee's measurement for whichever mosaic this is.
fn measure_radius_impl(raw: &RawImage) -> Option<f32> {
    if raw.cpp != 1 || raw.width < 16 || raw.height < 16 {
        // sRAW and linear DNGs: no mosaic, and RawTherapee does not capture
        // sharpen those at all. Manual radius still works.
        return None;
    }
    let black = raw
        .blacklevel
        .levels
        .first()
        .map(|r| r.as_f32())
        .unwrap_or(0.0);
    let white = *raw.whitelevel.0.first()? as f32;
    if white <= black {
        return None;
    }
    let area = picture_area(raw)?;
    let data = scaled_area(raw, &area, black, white)?;
    let (w, h) = (area.w, area.h);
    let cfa = &raw.camera.cfa;
    // The pattern as seen from the corner of the picture area, which is
    // where RawTherapee's rawData starts.
    let color = |row: usize, col: usize| cfa.color_at(row + area.y, col + area.x);
    let upper = 65535.0;

    let radius = if cfa.width == 6 && cfa.height == 6 {
        let (sy, sx) = xtrans_start(&color)?;
        calc_radius_xtrans(&data, w, h, 1000.0, upper, sy, sx)
    } else if cfa.width == 2 && cfa.height == 2 {
        let fc = [color(0, 0), color(1, 0)];
        calc_radius_bayer(&data, w, h, 1000.0, upper, fc)
    } else {
        return None;
    };
    // RawTherapee: std::min(calcRadius*(...), maxSigma), and gives up on NaN.
    if radius.is_nan() {
        return None;
    }
    Some(radius.min(MAX_AUTO_RADIUS))
}

/// The part of the sensor that holds the picture: the crop rawler recommends
/// if it has one, else the unmasked area. Measuring across the masked strips
/// round the edge would find the steepest "edge" on the sensor.
#[derive(Clone, Copy, Debug)]
struct Area {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
fn picture_area(raw: &RawImage) -> Option<Area> {
    let full = Area {
        x: 0,
        y: 0,
        w: raw.width,
        h: raw.height,
    };
    let area = raw
        .crop_area
        .or(raw.active_area)
        .map(|r| Area {
            x: r.p.x,
            y: r.p.y,
            w: r.d.w,
            h: r.d.h,
        })
        .unwrap_or(full);
    let fits = area.x + area.w <= raw.width && area.y + area.h <= raw.height;
    (fits && area.w >= 16 && area.h >= 16).then_some(area)
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// The picture area, black subtracted and scaled as RawTherapee's rawData is
/// at this point: green's white at 65535, which is what its limits - 1000
/// below, clipVal above - are in.
fn scaled_area(raw: &RawImage, area: &Area, black: f32, white: f32) -> Option<Vec<f32>> {
    let scale = 65535.0 / (white - black);
    let stride = raw.width;
    let sample = |i: usize| -> Option<f32> {
        let v = match &raw.data {
            RawImageData::Integer(px) => *px.get(i)? as f32,
            RawImageData::Float(px) => *px.get(i)?,
        };
        Some((v - black).max(0.0) * scale)
    };
    let mut out = Vec::with_capacity(area.w * area.h);
    for row in area.y..area.y + area.h {
        for col in area.x..area.x + area.w {
            out.push(sample(row * stride + col)?);
        }
    }
    Some(out)
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// rt_ calcRadiusBayer, RawTherapee capturesharpening.cc
/// @ c6d04960e14fed89e531cbbdf680021d131b4685.
///
/// The largest ratio between two diagonally adjacent green photosites,
/// ignoring clipped neighbourhoods, says how steep the sharpest edge in the
/// photo is - and so how wide a gaussian blurred it: sigma = sqrt(1 / ln(ratio)).
fn calc_radius_bayer(
    data: &[f32],
    w: usize,
    h: usize,
    lower: f32,
    upper: f32,
    fc: [usize; 2],
) -> f32 {
    (1.0 / bayer_peak(data, w, h, lower, upper, fc).ln()).sqrt()
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d (`bayer_peak`), less
// the position it also reports for the app's diagnostic test.
fn bayer_peak(data: &[f32], w: usize, h: usize, lower: f32, upper: f32, fc: [usize; 2]) -> f32 {
    let at = |r: usize, c: usize| data[r * w + c];
    (4..h.saturating_sub(4))
        .into_par_iter()
        .map(|row| {
            let mut ratio = 1.0f32;
            // RawTherapee: col = 5 + (fc[row & 1] & 1), stepping by two, which
            // lands on green on every row - its FC codes green as 1 or 3.
            let green_first = fc[row & 1] == 1 || fc[row & 1] == 3;
            let mut col = 5 + usize::from(green_first);
            while col < w - 4 {
                let val00 = at(row, col);
                if val00 > 0.0 {
                    let val1m1 = at(row + 1, col - 1);
                    let val1p1 = at(row + 1, col + 1);
                    let max_val0 = val00.max(val1m1);
                    if val1m1 > 0.0 && max_val0 > lower {
                        let min_val = val00.min(val1m1);
                        if max_val0 > ratio * min_val {
                            let clipped = if max_val0 == val00 {
                                at(row - 1, col - 1).max(at(row - 1, col + 1)).max(val1p1) >= upper
                            } else {
                                at(row, col - 2)
                                    .max(val00)
                                    .max(at(row + 2, col - 2))
                                    .max(at(row + 2, col))
                                    >= upper
                            };
                            if !clipped {
                                ratio = max_val0 / min_val;
                            }
                        }
                    }
                    let max_val1 = val00.max(val1p1);
                    if val1p1 > 0.0 && max_val1 > lower {
                        let min_val = val00.min(val1p1);
                        if max_val1 > ratio * min_val {
                            let clipped = if max_val1 == val00 {
                                at(row - 1, col - 1).max(at(row - 1, col + 1)).max(val1p1) >= upper
                            } else {
                                val00
                                    .max(at(row, col + 2))
                                    .max(at(row + 2, col))
                                    .max(at(row + 2, col + 2))
                                    >= upper
                            };
                            if !clipped {
                                ratio = max_val1 / min_val;
                            }
                        }
                    }
                }
                col += 2;
            }
            ratio
        })
        .reduce(|| 1.0, |a, b| if b > a { b } else { a })
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// Where RawTherapee starts its X-Trans scan: in rows and columns 6..12, a
/// green whose left and right neighbours differ, with no green above it and
/// none to its left. As `getDeconvAutoRadius_capturesharpening_SE` finds it.
fn xtrans_start(color: &impl Fn(usize, usize) -> usize) -> Option<(usize, usize)> {
    for i in 6..12 {
        for j in 6..12 {
            if color(i, j) == 1
                && color(i, j - 1) != color(i, j + 1)
                && color(i - 1, j) != 1
                && color(i, j - 1) != 1
            {
                return Some((i - 6, j - 6));
            }
        }
    }
    None
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// rt_ calcRadiusXtrans, RawTherapee capturesharpening.cc
/// @ c6d04960e14fed89e531cbbdf680021d131b4685. The same measurement as the
/// Bayer one, on the pairs of greens an X-Trans pattern has.
fn calc_radius_xtrans(
    data: &[f32],
    w: usize,
    h: usize,
    lower: f32,
    upper: f32,
    starty: usize,
    startx: usize,
) -> f32 {
    let at = |r: usize, c: usize| data[r * w + c];
    let rows: Vec<usize> = (starty + 2..h.saturating_sub(4)).step_by(3).collect();
    let max_ratio = rows
        .into_par_iter()
        .map(|row| {
            let mut max_ratio = 1.0f32;
            let mut consider = |a: f32, b: f32| {
                let max_val = a.max(b);
                if max_val > lower {
                    let min_val = a.min(b);
                    if max_val > max_ratio * min_val {
                        max_ratio = max_val / min_val;
                    }
                }
            };
            let mut col = startx + 2;
            while col < w.saturating_sub(4) {
                let valp1p1 = at(row + 1, col + 1);
                let square_clipped = valp1p1
                    .max(at(row + 1, col + 2))
                    .max(at(row + 2, col + 1))
                    .max(at(row + 2, col + 2))
                    >= upper;
                let green_solitary = at(row, col);
                if green_solitary > 1.0
                    && green_solitary < upper
                    && at(row - 1, col - 1).max(at(row - 1, col + 1)) < upper
                {
                    let valp1m1 = at(row + 1, col - 1);
                    if valp1m1 > 1.0
                        && at(row + 1, col - 2)
                            .max(valp1m1)
                            .max(at(row + 2, col - 2))
                            .max(at(row + 1, col - 1))
                            < upper
                    {
                        consider(green_solitary, valp1m1);
                    }
                    if valp1p1 > 1.0 && !square_clipped {
                        consider(green_solitary, valp1p1);
                    }
                }
                if !square_clipped {
                    let valp2p2 = at(row + 2, col + 2);
                    if valp2p2 > 1.0 {
                        if valp1p1 > 1.0 {
                            consider(valp1p1, valp2p2);
                        }
                        let right = at(row + 3, col + 3);
                        if right.max(at(row + 4, col + 2)).max(at(row + 4, col + 4)) < upper
                            && right > 1.0
                        {
                            consider(right, valp2p2);
                        }
                    }
                    let valp1p2 = at(row + 1, col + 2);
                    let valp2p1 = at(row + 2, col + 1);
                    if valp2p1 > 1.0 {
                        if valp1p2 > 1.0 {
                            consider(valp1p2, valp2p1);
                        }
                        let left = at(row + 3, col);
                        if left.max(at(row + 4, col - 1)).max(at(row + 4, col + 1)) < upper
                            && left > 1.0
                        {
                            consider(left, valp2p1);
                        }
                    }
                }
                col += 3;
            }
            max_ratio
        })
        .reduce(|| 1.0, f32::max);
    (1.0 / max_ratio.ln()).sqrt()
}

// ---------------------------------------------------------------------------
// Auto contrast: from the flattest part of the photo. Copied from
// src-tauri/src/mods/sharpen.rs @ 2bcb953d.
// ---------------------------------------------------------------------------

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// RawTherapee's L, which is L* scaled to 0..32768. Its constants below are in
/// these units, so the luminance is too.
fn rt_l(r: f32, g: f32, b: f32) -> f32 {
    let y = (0.212671 * r + 0.715160 * g + 0.072169 * b).max(0.0);
    let e = 216.0 / 24389.0;
    let k = 24389.0 / 27.0;
    let f = if y > e {
        y.cbrt()
    } else {
        (k * y + 16.0) / 116.0
    };
    (116.0 * f - 16.0) * 327.68
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// rt_ calcBlendFactor, RawTherapee rt_algo.cc.
fn blend_factor(val: f32, threshold: f32) -> f32 {
    let x = -16.0 + (16.0 / threshold) * val;
    0.5 * (1.0 + x / (1.0 + x * x).sqrt())
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
fn tile_average(l: &[f32], w: usize, y0: usize, x0: usize, size: usize) -> f32 {
    let mut sum = 0.0f32;
    for y in y0..y0 + size {
        sum += l[y * w + x0..y * w + x0 + size].iter().sum::<f32>();
    }
    sum / (size * size) as f32
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
fn tile_variance(l: &[f32], w: usize, y0: usize, x0: usize, size: usize, avg: f32) -> f32 {
    let mut var = 0.0f32;
    for y in y0..y0 + size {
        for &v in &l[y * w + x0..y * w + x0 + size] {
            var += (v - avg) * (v - avg);
        }
    }
    var / ((size * size) as f32 * avg)
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// rt_ calcContrastThreshold: the lowest threshold at which no more than 1%
/// of this flat tile would be sharpened.
fn calc_contrast_threshold(l: &[f32], w: usize, tile_y: usize, tile_x: usize, size: usize) -> f32 {
    const SCALE: f32 = 0.0625 / 327.68;
    let at = |y: usize, x: usize| l[y * w + x];
    let mut contrast = Vec::with_capacity((size - 4) * (size - 4));
    for j in tile_y + 2..tile_y + size - 2 {
        for i in tile_x + 2..tile_x + size - 2 {
            let c = ((at(j, i + 1) - at(j, i - 1)).powi(2)
                + (at(j + 1, i) - at(j - 1, i)).powi(2)
                + (at(j, i + 2) - at(j, i - 2)).powi(2)
                + (at(j + 2, i) - at(j - 2, i)).powi(2))
            .sqrt()
                * SCALE;
            contrast.push(c);
        }
    }
    let limit = ((size - 4) * (size - 4)) as f32 / 100.0;
    let mut c = 1;
    while c < 100 {
        let threshold = c as f32 / 100.0;
        let sum: f32 = contrast.iter().map(|&v| blend_factor(v, threshold)).sum();
        if sum <= limit {
            break;
        }
        c += 1;
    }
    (c + 1) as f32 / 100.0
}

// Copied from src-tauri/src/mods/sharpen.rs @ 2bcb953d.
/// rt_ the automatic half of buildBlendMask, RawTherapee rt_algo.cc
/// @ 1c2b3f33772eaf491b7bd4d66323ac36d13aa344.
///
/// Finds the flattest tile of mid-tone luminance - an 80 px pass, then a
/// finer 40 px pass round the best of it - and sets the threshold so that
/// tile is left almost entirely alone. Returns the threshold in 0..1, and 0
/// when nothing flat enough was found, which RawTherapee reads as "sharpen
/// everywhere".
fn auto_contrast_threshold(l: &[f32], w: usize, h: usize) -> f32 {
    const MIN_LUMINANCE: f32 = 2000.0;
    const MAX_LUMINANCE: f32 = 20000.0;
    const MIN_TILE_VARIANCE: f32 = 0.5;

    let variance_or_inf = |y: usize, x: usize, size: usize| {
        let avg = tile_average(l, w, y, x, size);
        if !(MIN_LUMINANCE..=MAX_LUMINANCE).contains(&avg) {
            return f32::INFINITY;
        }
        let v = tile_variance(l, w, y, x, size, avg);
        if v < MIN_TILE_VARIANCE {
            f32::INFINITY
        } else {
            v
        }
    };

    for pass in 0..2usize {
        let tilesize = 80 / (pass + 1);
        let skip = if pass == 0 { tilesize } else { tilesize / 4 };
        // RawTherapee's counts, which keep the last tile inside the image:
        // pass 0 steps a whole tile, pass 1 a quarter, less the three
        // quarter-steps a 40 px tile overhangs.
        let tiles_w = (w / skip).saturating_sub(3 * pass);
        let tiles_h = (h / skip).saturating_sub(3 * pass);
        if tiles_w == 0 || tiles_h == 0 {
            return 0.0;
        }
        let best = (0..tiles_h)
            .into_par_iter()
            .flat_map_iter(|i| (0..tiles_w).map(move |j| (i, j)))
            .map(|(i, j)| (variance_or_inf(i * skip, j * skip, tilesize), i, j))
            .reduce(
                || (f32::INFINITY, 0, 0),
                |a, b| if b.0 < a.0 { b } else { a },
            );
        let (minvar, min_i, min_j) = best;

        if minvar <= 1.0 || pass == 1 {
            let min_y = skip * min_i;
            let min_x = skip * min_j;
            if pass == 0 {
                return calc_contrast_threshold(l, w, min_y, min_x, tilesize);
            }
            // Second pass: every position within +-skip of the best tile.
            let y_start = min_y.saturating_sub(skip);
            let x_start = min_x.saturating_sub(skip);
            let y_end = (min_y + skip).min(h - tilesize);
            let x_end = (min_x + skip).min(w - tilesize);
            let mut best = (f32::INFINITY, 0, 0);
            for y in y_start..=y_end {
                for x in x_start..=x_end {
                    let v = variance_or_inf(y, x, tilesize);
                    if v < best.0 {
                        best = (v, y, x);
                    }
                }
            }
            return if best.0 <= 8.0 {
                calc_contrast_threshold(l, w, best.1, best.2, tilesize)
            } else {
                0.0
            };
        }
    }
    0.0
}

// ---------------------------------------------------------------------------
// The passes. Copied from src-tauri/src/mods/sharpen_gpu.rs @ 2bcb953d,
// capture sharpening only.
// ---------------------------------------------------------------------------

const TILE: u32 = 1024;
const BORDER: u32 = 64;
const EXT: u32 = TILE + 2 * BORDER;
const STOP_BLOCK: u32 = 32;
const WORKGROUP: u32 = 16;

/// Below this sigma, in render pixels, a gaussian is a delta function to the
/// eye: capture sharpening at fit-to-screen. Skipped rather than computed.
const MIN_SIGMA: f32 = 0.25;

/// RawTherapee's clip mask level, 0.95 of white.
const CLIP_LEVEL: f32 = 0.95;

// Flags the shader reads. Match the constants at the top of the .wgsl, and
// the app's values for the same flags.
const FLAG_CAPTURE: u32 = 1;
const FLAG_ITER_CHECK: u32 = 8;
const FLAG_MASK: u32 = 16;
const FLAG_CLIP: u32 = 64;

pub const SHADER: &str = include_str!("rt_capture_sharpen.wgsl");

/// `Params` in the shader, field for field.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug, PartialEq)]
struct Uniform {
    img_w: u32,
    img_h: u32,
    origin_x: i32,
    origin_y: i32,
    ext_w: u32,
    ext_h: u32,
    inner_x0: u32,
    inner_y0: u32,
    inner_w: u32,
    inner_h: u32,
    flags: u32,
    contrast: f32,
    mask_sigma: f32,
    cap_sigma: f32,
    cap_slope: f32,
    cap_amount: f32,
    centre_x: f32,
    centre_y: f32,
    clip: f32,
    _pad0: u32,
}

/// What is going to run, worked out before anything touches the GPU.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Plan {
    flags: u32,
    iterations: u32,
    contrast: f32,
    mask_sigma: f32,
    cap_sigma: f32,
    cap_slope: f32,
    cap_amount: f32,
}

// Copied from src-tauri/src/mods/sharpen_gpu.rs @ 2bcb953d (`plan_for`), the
// capture sharpening half.
fn plan(p: &Resolved, width: usize, height: usize, px_scale: f32) -> Option<Plan> {
    let s = px_scale.clamp(1.0e-3, 1.0);
    let (w, h) = (width as f32, height as f32);

    // RawTherapee's corner boost: the radius at the corners is
    // min(2, radius + boost), reached linearly from the centre.
    let cap_sigma = p.radius * s;
    let corner_sigma = (p.radius + p.corner).min(2.0) * s;
    let corner_distance = ((w * 0.5).powi(2) + (h * 0.5).powi(2)).sqrt().max(1.0);
    let cap_slope = (corner_sigma - cap_sigma) / corner_distance;
    let capture = p.amount > 0.0 && p.iterations > 0 && cap_sigma.max(corner_sigma) >= MIN_SIGMA;
    if !capture {
        return None;
    }

    let mut flags = FLAG_CAPTURE;
    if p.iter_check {
        flags |= FLAG_ITER_CHECK;
    }
    // The mask, with its clip guard, unless the threshold is 0.
    if p.contrast > 0.0 {
        flags |= FLAG_MASK | FLAG_CLIP;
    }
    Some(Plan {
        flags,
        iterations: p.iterations.clamp(1, 100),
        contrast: p.contrast,
        // RawTherapee blurs its mask with sigma 2, at full resolution.
        mask_sigma: (2.0 * s).max(0.3),
        cap_sigma,
        cap_slope,
        cap_amount: p.amount,
    })
}

struct Pipelines {
    prep: wgpu::ComputePipeline,
    mask_h: wgpu::ComputePipeline,
    mask_v: wgpu::ComputePipeline,
    tick: wgpu::ComputePipeline,
    rl_blur_est_h: wgpu::ComputePipeline,
    rl_ratio_v: wgpu::ComputePipeline,
    rl_blur_ratio_h: wgpu::ComputePipeline,
    rl_update_v: wgpu::ComputePipeline,
    capture_mix: wgpu::ComputePipeline,
    compose: wgpu::ComputePipeline,
}

/// Everything the passes need, made once per device.
struct Engine {
    gpu: Arc<Gpu>,
    /// OpenGL: small submissions, waited for without holding its context.
    gl: bool,
    layout: wgpu::BindGroupLayout,
    pipelines: Pipelines,
    uniform: wgpu::Buffer,
    scratch: [wgpu::Buffer; 5],
    stopped: wgpu::Buffer,
    counter: wgpu::Buffer,
}

impl Engine {
    // Copied from src-tauri/src/mods/sharpen_gpu.rs @ 2bcb953d (`Engine::new`),
    // with the bindings capture sharpening uses: the result is an r32float
    // texture of factors, and there is no mask array.
    fn new(gpu: &Arc<Gpu>) -> Self {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rt_capture_sharpen"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let storage = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt_capture_sharpen"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Uniform>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                storage(3),
                storage(4),
                storage(5),
                storage(6),
                storage(7),
                storage(8),
                storage(9),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt_capture_sharpen"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let pipelines = Pipelines {
            prep: pipeline("prep"),
            mask_h: pipeline("mask_h"),
            mask_v: pipeline("mask_v"),
            tick: pipeline("tick"),
            rl_blur_est_h: pipeline("rl_blur_est_h"),
            rl_ratio_v: pipeline("rl_ratio_v"),
            rl_blur_ratio_h: pipeline("rl_blur_ratio_h"),
            rl_update_v: pipeline("rl_update_v"),
            capture_mix: pipeline("capture_mix"),
            compose: pipeline("compose"),
        };

        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let plane = (EXT as u64) * (EXT as u64) * 4;
        let rw = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let blocks = (EXT.div_ceil(STOP_BLOCK) as u64).pow(2) * 4;

        Engine {
            gpu: Arc::clone(gpu),
            gl: gpu.adapter.backend == wgpu::Backend::Gl,
            layout,
            pipelines,
            uniform: buffer(
                "rt_capture_sharpen params",
                std::mem::size_of::<Uniform>() as u64,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            scratch: [
                buffer("rt_capture_sharpen y0", plane, rw),
                buffer("rt_capture_sharpen estimate", plane, rw),
                buffer("rt_capture_sharpen tmp", plane, rw),
                buffer("rt_capture_sharpen aux", plane, rw),
                buffer("rt_capture_sharpen blend", plane, rw),
            ],
            stopped: buffer("rt_capture_sharpen stopped", blocks, rw),
            counter: buffer("rt_capture_sharpen counter", 16, rw),
        }
    }

    /// The factor each pixel of `rgb` is to be multiplied by.
    fn ratios(&self, rgb: &Rgb, plan: &Plan) -> Result<Vec<f32>> {
        let device = &self.gpu.device;
        let (w, h) = (rgb.width as u32, rgb.height as u32);
        let limit = device.limits().max_texture_dimension_2d;
        if w > limit || h > limit {
            return Err(Error::Tool {
                tool: RtCaptureSharpen::ID,
                message: format!("{w}x{h} is past this GPU's texture limit of {limit}"),
            });
        }

        let src = self.upload(rgb);
        let ratio = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_capture_sharpen ratio"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let src_view = src.create_view(&Default::default());
        let ratio_view = ratio.create_view(&Default::default());

        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&src_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&ratio_view),
            },
        ];
        for (i, buffer) in self.scratch.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 3 + i as u32,
                resource: buffer.as_entire_binding(),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: 8,
            resource: self.stopped.as_entire_binding(),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 9,
            resource: self.counter.as_entire_binding(),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt_capture_sharpen"),
            layout: &self.layout,
            entries: &entries,
        });

        let base = Uniform {
            img_w: w,
            img_h: h,
            flags: plan.flags,
            contrast: plan.contrast,
            mask_sigma: plan.mask_sigma,
            cap_sigma: plan.cap_sigma,
            cap_slope: plan.cap_slope,
            cap_amount: plan.cap_amount,
            centre_x: w as f32 * 0.5,
            centre_y: h as f32 * 0.5,
            clip: CLIP_LEVEL,
            ..Default::default()
        };

        // Copied from src-tauri/src/mods/sharpen_gpu.rs @ 2bcb953d (`encode`):
        // tiles of TILE with a BORDER, over the whole picture.
        let mut ty = 0;
        while ty < h {
            let th = TILE.min(h - ty);
            let mut tx = 0;
            while tx < w {
                let tw = TILE.min(w - tx);
                let ex0 = tx.saturating_sub(BORDER);
                let ey0 = ty.saturating_sub(BORDER);
                let ex1 = (tx + tw + BORDER).min(w);
                let ey1 = (ty + th + BORDER).min(h);
                let uniform = Uniform {
                    origin_x: ex0 as i32,
                    origin_y: ey0 as i32,
                    ext_w: ex1 - ex0,
                    ext_h: ey1 - ey0,
                    inner_x0: tx - ex0,
                    inner_y0: ty - ey0,
                    inner_w: tw,
                    inner_h: th,
                    ..base
                };
                self.tile(&bind_group, &uniform, plan);
                tx += TILE;
            }
            ty += TILE;
        }

        self.read(&ratio, w, h)
    }

    /// `rgb` as an rgba32float texture, a band of rows at a time so the
    /// staging copy stays small.
    fn upload(&self, rgb: &Rgb) -> wgpu::Texture {
        let (w, h) = (rgb.width as u32, rgb.height as u32);
        let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_capture_sharpen input"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let rows = band_rows(w as usize * 16);
        for y0 in (0..h).step_by(rows) {
            let n = rows.min((h - y0) as usize);
            let start = y0 as usize * rgb.width;
            let band: Vec<f32> = rgb.pixels[start..start + n * rgb.width]
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 1.0])
                .collect();
            self.gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: y0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&band),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 16),
                    rows_per_image: Some(n as u32),
                },
                wgpu::Extent3d {
                    width: w,
                    height: n as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        texture
    }

    /// The factors back from the GPU, a band of rows at a time.
    fn read(&self, texture: &wgpu::Texture, w: u32, h: u32) -> Result<Vec<f32>> {
        let device = &self.gpu.device;
        let row = (w * 4).div_ceil(256) * 256;
        let rows = band_rows(row as usize) as u32;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt_capture_sharpen readback"),
            size: (row * rows.min(h)) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut out = Vec::with_capacity((w * h) as usize);
        for y0 in (0..h).step_by(rows as usize) {
            let n = rows.min(h - y0);
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: y0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row),
                        rows_per_image: Some(n),
                    },
                },
                wgpu::Extent3d {
                    width: w,
                    height: n,
                    depth_or_array_layers: 1,
                },
            );
            self.gpu.queue.submit(Some(encoder.finish()));
            let slice = buffer.slice(..(row * n) as u64);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(Duration::from_secs(120)),
                })
                .map_err(|e| self.failed(e))?;
            rx.recv()
                .map_err(|e| self.failed(e))?
                .map_err(|e| self.failed(e))?;
            {
                let bytes = slice.get_mapped_range();
                for r in 0..n as usize {
                    let line = &bytes[r * row as usize..r * row as usize + w as usize * 4];
                    out.extend_from_slice(bytemuck::cast_slice::<u8, f32>(line));
                }
            }
            buffer.unmap();
        }
        Ok(out)
    }

    fn failed(&self, e: impl std::fmt::Display) -> Error {
        Error::Tool {
            tool: RtCaptureSharpen::ID,
            message: format!("reading the result back: {e}"),
        }
    }

    // Copied from src-tauri/src/mods/sharpen_gpu.rs @ 2bcb953d (`Engine::tile`),
    // the capture sharpening steps and the last pass.
    fn tile(&self, bind_group: &wgpu::BindGroup, u: &Uniform, plan: &Plan) {
        let whole = (u.ext_w.div_ceil(WORKGROUP), u.ext_h.div_ceil(WORKGROUP));
        let one = (1, 1);
        let p = &self.pipelines;

        // The tile's work as steps, each a few dispatches. On Vulkan and DX12
        // they all go in one submission; on OpenGL each step is its own.
        let mut steps: Vec<Vec<(&wgpu::ComputePipeline, (u32, u32))>> = Vec::new();
        let mut first = vec![(&p.prep, whole)];
        if plan.flags & FLAG_MASK != 0 {
            first.push((&p.mask_h, whole));
            first.push((&p.mask_v, whole));
        }
        steps.push(first);
        for _ in 0..plan.iterations {
            steps.push(vec![
                (&p.tick, one),
                (&p.rl_blur_est_h, whole),
                (&p.rl_ratio_v, whole),
                (&p.rl_blur_ratio_h, whole),
                (&p.rl_update_v, whole),
            ]);
        }
        steps.push(vec![(&p.capture_mix, whole), (&p.compose, whole)]);

        let device = &self.gpu.device;
        let queue = &self.gpu.queue;
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(u));
        // On GL a few steps at a time: twelve iterations of a tile is tens of
        // milliseconds, well inside the second GL allows another thread.
        let chunks: Vec<Vec<_>> = if self.gl {
            steps
                .chunks(12)
                .map(|group| group.iter().flatten().copied().collect())
                .collect()
        } else {
            vec![steps.iter().flatten().copied().collect()]
        };
        let mut cleared = false;
        for chunk in &chunks {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rt_capture_sharpen tile"),
            });
            if !cleared {
                encoder.clear_buffer(&self.stopped, 0, None);
                encoder.clear_buffer(&self.counter, 0, None);
                cleared = true;
            }
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("rt_capture_sharpen"),
                    timestamp_writes: None,
                });
                pass.set_bind_group(0, bind_group, &[]);
                for (pipeline, groups) in chunk {
                    pass.set_pipeline(pipeline);
                    pass.dispatch_workgroups(groups.0, groups.1, 1);
                }
            }
            let index = queue.submit(Some(encoder.finish()));
            if self.gl {
                wait_briefly(device, index);
            }
        }
    }
}

/// Rows per band for a row of `row_bytes`, so a band is about 64 MB.
fn band_rows(row_bytes: usize) -> usize {
    ((64 << 20) / row_bytes.max(1)).max(1)
}

// Copied from src-tauri/src/mods/sharpen_gpu.rs @ 2bcb953d.
/// Wait for one submission on OpenGL without holding its context long: a
/// blocking poll on GL keeps the one context lock until the GPU is done, and
/// every other thread that wants it panics after a second.
fn wait_briefly(device: &wgpu::Device, index: wgpu::SubmissionIndex) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(30) {
        match device.poll(wgpu::PollType::Wait {
            submission_index: Some(index.clone()),
            timeout: Some(Duration::from_millis(200)),
        }) {
            Err(wgpu::PollError::Timeout) => continue,
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved() -> Resolved {
        Resolved {
            amount: 1.0,
            radius: DEFAULT_RADIUS,
            corner: 0.0,
            iterations: DEFAULT_ITERATIONS,
            iter_check: true,
            contrast: 0.1,
        }
    }

    #[test]
    fn the_uniform_matches_the_shader_struct() {
        // 20 four-byte fields in the shader's Params.
        assert_eq!(std::mem::size_of::<Uniform>(), 80);
    }

    #[test]
    fn the_shader_compiles() {
        let module = match wgpu::naga::front::wgsl::parse_str(SHADER) {
            Ok(module) => module,
            Err(e) => panic!("does not parse:\n{}", e.emit_to_string(SHADER)),
        };
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("does not validate:\n{}", e.emit_to_string(SHADER));
        }
    }

    // The app's own checks on `plan_for`, for the half that came across.

    #[test]
    fn capture_sharpening_is_skipped_at_fit_to_screen_and_runs_at_100_percent() {
        // 1920 px of a 6000 px photo: 0.75 * 0.32 = 0.24 px of blur.
        assert!(plan(&resolved(), 6000, 4000, 0.32).is_none());
        let full = plan(&resolved(), 6000, 4000, 1.0).expect("runs at 1:1");
        assert_eq!(full.flags & FLAG_CAPTURE, FLAG_CAPTURE);
        assert_eq!(full.flags & FLAG_ITER_CHECK, FLAG_ITER_CHECK);
        assert_eq!(full.iterations, 20);
        assert_eq!(full.cap_sigma, 0.75);
        assert_eq!(full.mask_sigma, 2.0);
    }

    #[test]
    fn corner_boost_reaches_its_radius_at_the_corner() {
        let corner = (3000.0f32.powi(2) + 2000.0f32.powi(2)).sqrt();
        let p = plan(
            &Resolved {
                corner: 0.4,
                ..resolved()
            },
            6000,
            4000,
            1.0,
        )
        .unwrap();
        assert!((p.cap_sigma + p.cap_slope * corner - 1.15).abs() < 1e-4);
        let p = plan(
            &Resolved {
                radius: 1.9,
                corner: 0.5,
                ..resolved()
            },
            6000,
            4000,
            1.0,
        )
        .unwrap();
        assert!((p.cap_sigma + p.cap_slope * corner - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_threshold_of_zero_has_no_mask() {
        let p = plan(
            &Resolved {
                contrast: 0.0,
                ..resolved()
            },
            600,
            400,
            1.0,
        )
        .unwrap();
        assert_eq!(p.flags & (FLAG_MASK | FLAG_CLIP), 0);
        assert!(
            plan(
                &Resolved {
                    amount: 0.0,
                    ..resolved()
                },
                600,
                400,
                1.0
            )
            .is_none()
        );
    }

    #[test]
    fn the_blend_factor_is_one_half_at_the_threshold() {
        assert!((blend_factor(0.1, 0.1) - 0.5).abs() < 1e-6);
        assert!(blend_factor(0.0, 0.1) < 0.01);
        assert!(blend_factor(0.3, 0.1) > 0.99);
    }
}
