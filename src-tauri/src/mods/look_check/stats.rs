//! What one render looks like, in numbers small enough to keep in git: the
//! luminance's quantiles, each channel's mean, what is clipped, how the top
//! three stops are filled, how colourful the blown area is, and a thumbnail.
//!
//! Everything is measured on the whole render, at the size the look check
//! renders (1600 pixels on the long side); only the thumbnail is small.

use argentum_engine::colour::{lab, luma, srgb_decode, srgb_encode};
use image::Rgba32FImage;

/// Where a channel counts as blown, on sRGB-encoded values: `AG_CLIP_HIGH` in
/// `shaders/modules.wgsl`, the line every clipping view draws.
pub const CLIP_HIGH: f32 = 0.998;

/// The luminance quantiles kept, as fractions of the frame.
pub const QUANTILES: [f64; 8] = [0.01, 0.05, 0.25, 0.5, 0.75, 0.95, 0.99, 0.999];

/// Which of `QUANTILES` is the 99.9th percentile.
pub const P999: usize = 7;

/// Which of `QUANTILES` is the median.
pub const P50: usize = 3;

/// Quarter-stop bins over the top three stops of linear luminance, brightest
/// first: bin `k` holds `2^-(k+1)/4 < Y <= 2^-k/4`, and bin 0 also everything
/// above white.
pub const TOP_BINS: usize = 12;

/// The thumbnail's long side, in cells.
pub const THUMB_LONG: u32 = 32;

/// A render's numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub stats: Stats,
    pub thumb: Thumb,
}

/// Everything but the thumbnail: what an anchor keeps.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    /// Linear luminance at each of `QUANTILES`.
    pub luma: [f64; QUANTILES.len()],
    /// Each channel's mean, sRGB-encoded as displayed.
    pub means: [f64; 3],
    /// The share of the frame above `CLIP_HIGH`, per channel.
    pub clipped: [f64; 3],
    /// The share of the frame in each of `TOP_BINS`.
    pub top: [f64; TOP_BINS],
    /// The share of the frame with any channel above `CLIP_HIGH`.
    pub blown: f64,
    /// The mean CIELAB chroma C* over that share; 0 when there is none.
    pub blown_chroma: f64,
}

/// The render's mean colour over a grid of cells: each cell's mean in linear
/// light, sRGB-encoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<[f32; 3]>,
}

/// The thumbnail's size for a render of `width` x `height`: `THUMB_LONG`
/// cells on the long side, the short side in proportion.
pub fn thumb_size(width: u32, height: u32) -> (u32, u32) {
    let short = |long: u32, short: u32| {
        ((THUMB_LONG as f64 * short as f64 / long as f64).round() as u32).max(1)
    };
    if width >= height {
        (THUMB_LONG, short(width, height))
    } else {
        (short(height, width), THUMB_LONG)
    }
}

pub fn measure(image: &Rgba32FImage) -> Look {
    let (width, height) = image.dimensions();
    let n = (width as usize * height as usize).max(1);
    let pixels: Vec<[f32; 3]> = image.pixels().map(|p| [p[0], p[1], p[2]]).collect();

    let mut lumas: Vec<f32> = Vec::with_capacity(n);
    let mut sums = [0.0f64; 3];
    let mut clipped = [0usize; 3];
    let mut top = [0usize; TOP_BINS];
    let (mut blown, mut blown_chroma) = (0usize, 0.0f64);
    for &shown in &pixels {
        let linear = shown.map(srgb_decode);
        let y = luma(linear);
        lumas.push(y);
        for c in 0..3 {
            sums[c] += shown[c] as f64;
            if shown[c] > CLIP_HIGH {
                clipped[c] += 1;
            }
        }
        if let Some(bin) = top_bin(y) {
            top[bin] += 1;
        }
        if shown.iter().any(|&v| v > CLIP_HIGH) {
            blown += 1;
            let [_, a, b] = lab(linear.map(|v| v.clamp(0.0, 1.0)));
            blown_chroma += a.hypot(b);
        }
    }
    lumas.sort_by(f32::total_cmp);
    let quantile = |q: f64| {
        let i = ((q * n as f64).ceil() as usize).clamp(1, lumas.len().max(1)) - 1;
        lumas.get(i).copied().unwrap_or(0.0) as f64
    };
    let share = |count: usize| count as f64 / n as f64;

    Look {
        stats: Stats {
            luma: QUANTILES.map(quantile),
            means: sums.map(|s| s / n as f64),
            clipped: clipped.map(share),
            top: top.map(share),
            blown: share(blown),
            blown_chroma: if blown > 0 {
                blown_chroma / blown as f64
            } else {
                0.0
            },
        },
        thumb: thumbnail(&pixels, width, height),
    }
}

/// Steps a thumbnail cell is kept in, per channel: tenths of an 8-bit level.
pub const CELL_STEPS: f32 = 2550.0;

/// A cell's value as its reference keeps it, so a render compared with its
/// own reference moves by nothing at all. Unrounded, the rounding alone came
/// to CIEDE2000 0.1 in the darkest cells.
pub fn shown_as_kept(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * CELL_STEPS).round() / CELL_STEPS
}

/// Which of `TOP_BINS` a linear luminance falls in, if any.
pub fn top_bin(y: f32) -> Option<usize> {
    if y <= 0.0 {
        return None;
    }
    let stops_below_white = (-(y.min(1.0)).log2()).max(0.0);
    let bin = (stops_below_white * 4.0) as usize;
    (bin < TOP_BINS).then_some(bin)
}

fn thumbnail(pixels: &[[f32; 3]], width: u32, height: u32) -> Thumb {
    let (tw, th) = thumb_size(width, height);
    let span = |cell: u32, cells: u32, size: u32| {
        let a = (cell as u64 * size as u64 / cells as u64) as u32;
        let b = ((cell as u64 + 1) * size as u64 / cells as u64) as u32;
        a..b.max(a + 1).min(size)
    };
    let mut cells = Vec::with_capacity((tw * th) as usize);
    for ty in 0..th {
        for tx in 0..tw {
            let (ys, xs) = (span(ty, th, height), span(tx, tw, width));
            let mut sum = [0.0f64; 3];
            let mut count = 0usize;
            for y in ys {
                for x in xs.clone() {
                    let p = pixels[y as usize * width as usize + x as usize];
                    for c in 0..3 {
                        sum[c] += srgb_decode(p[c].clamp(0.0, 1.0)) as f64;
                    }
                    count += 1;
                }
            }
            cells.push(sum.map(|s| shown_as_kept(srgb_encode((s / count.max(1) as f64) as f32))));
        }
    }
    Thumb {
        width: tw,
        height: th,
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clip_line_is_the_shaders() {
        let shader = include_str!("../../shaders/modules.wgsl");
        assert!(
            shader.contains(&format!("const AG_CLIP_HIGH: f32 = {CLIP_HIGH};")),
            "AG_CLIP_HIGH in modules.wgsl is no longer {CLIP_HIGH}: change CLIP_HIGH with it"
        );
    }

    #[test]
    fn the_top_bins_are_quarter_stops_from_white() {
        assert_eq!(top_bin(1.0), Some(0));
        assert_eq!(top_bin(4.0), Some(0), "above white counts as white");
        assert_eq!(top_bin(0.5f32.powf(0.24)), Some(0));
        assert_eq!(top_bin(0.5f32.powf(0.26)), Some(1));
        assert_eq!(top_bin(0.5), Some(4));
        assert_eq!(top_bin(0.126), Some(11));
        assert_eq!(top_bin(0.124), None, "below the top three stops");
        assert_eq!(top_bin(0.0), None);
    }

    #[test]
    fn a_thumbnail_is_thirty_two_cells_on_the_long_side() {
        assert_eq!(thumb_size(1600, 1067), (32, 21));
        assert_eq!(thumb_size(1600, 1200), (32, 24));
        assert_eq!(thumb_size(1600, 900), (32, 18));
        assert_eq!(thumb_size(1067, 1600), (21, 32));
    }

    #[test]
    fn a_flat_grey_measures_as_itself() {
        let grey = 0.5f32;
        let image = Rgba32FImage::from_pixel(64, 48, image::Rgba([grey, grey, grey, 1.0]));
        let look = measure(&image);
        let y = srgb_decode(grey) as f64;
        assert!(look.stats.luma.iter().all(|q| (q - y).abs() < 1e-6));
        assert!(look.stats.means.iter().all(|m| (m - 0.5).abs() < 1e-6));
        assert_eq!(look.stats.clipped, [0.0; 3]);
        assert_eq!(look.stats.blown, 0.0);
        assert_eq!(look.thumb.cells.len(), 32 * 24);
        assert!(
            look.thumb
                .cells
                .iter()
                .all(|c| c.iter().all(|v| (v - grey).abs() < 1e-5))
        );
    }
}
