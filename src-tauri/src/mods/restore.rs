//! Restore: undo lens softness with OmniLens, only where the photo has detail.
//!
//! A wide-open lens is soft from its aberrations, not from missed focus, and
//! the softness is the same at every size, so it is undone at the photo's own
//! size, before Capture sharpening - which then sharpens edges that are there
//! rather than haze. It runs from the Super Resolution window, alone or before
//! an enlargement; `super_resolution.rs` runs the model and saves the file.
//!
//! THE MODEL
//!
//! OmniLens (Jiang et al., "OmniLens: Towards Universal Lens Aberration
//! Correction via LensLib-to-Specific Domain Adaptation"), the SwinIR network
//! pretrained on its AODLib-EAOD lens library: thousands of simulated lens
//! designs, so it corrects aberrations blindly, for a lens it has never seen.
//! github.com/zju-jiangqi/OmniLens `basicsr/archs/swinir_arch.py`
//! @ 89dcbc7ff7e79edc6aea2b58f83750d575c2f4bf, weights
//! huggingface.co/datasets/Strange97/OmniLens-PretrainedModel
//! `SwinIR_AODLib_EAOD.pth` @ fb0ded25e63fc2c18f8f59e5519e3a659ea796a7, both
//! Apache-2.0. Converted to ONNX at a fixed
//! 256 px tile (its window attention fixes the shape) and published in
//! github.com/akhater/argentum-models, so Argentum's own download stays
//! small. Measured against the PyTorch original on random tiles: identical.
//!
//! WHY A MASK
//!
//! It was trained on clean renders, so it reads sensor grain as aberration and
//! sharpens it: soft backgrounds turn to grit. Measured on R6 Mark III
//! 400 mm frames, 2026-10-11. So the result is blended in by local contrast -
//! the shape of capture sharpening's contrast mask, RawTherapee
//! `rtengine/rt_algo.cc` - and smooth areas keep the original. The slider
//! moves the threshold. The tight end is the default: the loose end
//! sharpens fur further but makes the noise look painted.

use std::sync::{Arc, Mutex};

use image::{Rgb32FImage, RgbImage};
use rayon::prelude::*;

/// The Mask slider's default: only real edges and texture.
pub const DEFAULT_MASK: f32 = 60.0;

/// The local-contrast window where the mask goes from off to fully on, for
/// slider position `setting` (0..100). Linear in the setting, so 0 is the
/// whole photo, and 60 lands on the pair tuned by eye on test frames
/// (0.012, 0.035 of sRGB-encoded luminance gradient after a 1.2 px blur).
fn thresholds(setting: f32) -> (f32, f32) {
    let s = setting.clamp(0.0, 100.0);
    (0.0002 * s, 0.00058 * s)
}

/// Blend `restored` into `source` through the detail mask for `setting`.
/// Both are the sRGB-encoded framed photo the model saw, the same size. The
/// window keeps the photo's detail and calls `blend_through`; this is the
/// whole of it in one call, for the tests.
#[cfg(test)]
pub fn blend(source: &Rgb32FImage, restored: &Rgb32FImage, setting: f32) -> Rgb32FImage {
    blend_through(source, restored, &detail_mask(source, setting))
}

/// The same, with the mask already made.
pub fn blend_through(source: &Rgb32FImage, restored: &Rgb32FImage, mask: &[f32]) -> Rgb32FImage {
    let mut out = source.clone();
    out.par_chunks_mut(3)
        .zip(restored.par_chunks(3))
        .zip(mask.par_iter())
        .for_each(|((o, r), m)| {
            for c in 0..3 {
                o[c] += (r[c] - o[c]) * m;
            }
        });
    out
}

/// 0..1 per pixel: how much of the restored photo to take.
pub fn detail_mask(image: &Rgb32FImage, setting: f32) -> Vec<f32> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    mask_from_detail(&detail(image), w, h, setting)
}

/// The mask for `setting`, from a photo's `detail`. The slow part of the mask
/// does not depend on the slider, so the window keeps it and this is all a
/// slider move costs.
pub fn mask_from_detail(detail: &[f32], w: usize, h: usize, setting: f32) -> Vec<f32> {
    let (low, high) = thresholds(setting);
    if high <= 1e-6 {
        return vec![1.0; w * h];
    }
    let mut g: Vec<f32> = detail
        .par_iter()
        .map(|v| {
            let t = ((*v - low) / (high - low)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect();
    gaussian_blur(&mut g, w, h, 1.5);
    g
}

/// Local contrast: the gradient of a lightly blurred luminance, blurred.
pub fn detail(image: &Rgb32FImage) -> Vec<f32> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut y: Vec<f32> = image
        .par_chunks(3)
        .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
        .collect();
    // So grain on its own does not count as detail.
    gaussian_blur(&mut y, w, h, 1.2);
    let mut g = vec![0.0f32; w * h];
    g.par_chunks_mut(w).enumerate().for_each(|(row, out)| {
        let base = row * w;
        for x in 0..w {
            let here = y[base + x];
            let dx = if x + 1 < w {
                y[base + x + 1] - here
            } else {
                0.0
            };
            let dy = if row + 1 < h {
                y[base + w + x] - here
            } else {
                0.0
            };
            out[x] = (dx * dx + dy * dy).sqrt();
        }
    });
    gaussian_blur(&mut g, w, h, 2.0);
    g
}

fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let radius = (sigma * 3.0).ceil().max(1.0) as i32;
    let mut k: Vec<f32> = (-radius..=radius)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= sum);
    k
}

/// Separable gaussian, edges clamped.
fn gaussian_blur(plane: &mut [f32], w: usize, h: usize, sigma: f32) {
    let k = gaussian_kernel(sigma);
    let r = (k.len() / 2) as isize;
    let mut tmp = vec![0.0f32; w * h];
    tmp.par_chunks_mut(w).enumerate().for_each(|(row, out)| {
        let src = &plane[row * w..(row + 1) * w];
        for (x, o) in out.iter_mut().enumerate() {
            let mut acc = 0.0;
            for (i, kv) in k.iter().enumerate() {
                let sx = (x as isize + i as isize - r).clamp(0, w as isize - 1) as usize;
                acc += src[sx] * kv;
            }
            *o = acc;
        }
    });
    plane.par_chunks_mut(w).enumerate().for_each(|(row, out)| {
        for (x, o) in out.iter_mut().enumerate() {
            let mut acc = 0.0;
            for (i, kv) in k.iter().enumerate() {
                let sy = (row as isize + i as isize - r).clamp(0, h as isize - 1) as usize;
                acc += tmp[sy * w + x] * kv;
            }
            *o = acc;
        }
    });
}

/// The model's output for the last photo restored, so moving the Mask slider
/// and pressing Enhance again does not run the model a second time. One photo
/// only: a 32 MP frame is about 380 MB of floats.
struct Kept {
    key: blake3::Hash,
    restored: Arc<Rgb32FImage>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

/// What identifies a framed photo: its pixels. A new crop, patch or decode
/// is a different photo to the model.
pub fn key_of(source: &Rgb32FImage) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&source.width().to_le_bytes());
    hasher.update(&source.height().to_le_bytes());
    hasher.update(bytemuck::cast_slice(source.as_raw()));
    hasher.finalize()
}

pub fn kept(key: &blake3::Hash) -> Option<Arc<Rgb32FImage>> {
    let slot = KEPT.lock().ok()?;
    slot.as_ref()
        .filter(|k| &k.key == key)
        .map(|k| k.restored.clone())
}

pub fn keep(key: blake3::Hash, restored: Arc<Rgb32FImage>) {
    if let Ok(mut slot) = KEPT.lock() {
        *slot = Some(Kept { key, restored });
    }
}

/// The photo the window is showing, framed as it will be restored, with its
/// detail: so the Mask view, every slider move and Enhance answer without
/// decoding the RAW again.
pub struct Framed {
    pub path: String,
    pub source: Arc<Rgb32FImage>,
    /// The decoded photo's size before framing, which masks are placed in.
    pub source_size: (u32, u32),
    pub detail: Arc<Vec<f32>>,
    /// `key_of(source)`, which finds the model's kept output.
    pub key: blake3::Hash,
}

/// The photo the window has open, if it has loaded one.
pub fn current_framed() -> Option<Arc<Framed>> {
    FRAMED.lock().ok()?.clone()
}

static FRAMED: Mutex<Option<Arc<Framed>>> = Mutex::new(None);

/// The framed photo for `path`, loaded with `load` the first time.
pub fn framed(
    path: &str,
    load: impl FnOnce() -> anyhow::Result<(Rgb32FImage, (u32, u32))>,
) -> anyhow::Result<Arc<Framed>> {
    if let Ok(slot) = FRAMED.lock()
        && let Some(framed) = slot.as_ref().filter(|f| f.path == path)
    {
        return Ok(framed.clone());
    }
    let (source, source_size) = load()?;
    let framed = Arc::new(Framed {
        path: path.to_string(),
        detail: Arc::new(detail(&source)),
        key: key_of(&source),
        source: Arc::new(source),
        source_size,
    });
    if let Ok(mut slot) = FRAMED.lock() {
        *slot = Some(framed.clone());
    }
    Ok(framed)
}

/// Let the kept results go: the window closed, or a file was saved.
pub fn forget() {
    if let Ok(mut slot) = KEPT.lock() {
        *slot = None;
    }
    if let Ok(mut slot) = FRAMED.lock() {
        *slot = None;
    }
}

/// For mods::memory's report. Zero if a slot is busy.
pub fn kept_bytes() -> usize {
    let kept = KEPT
        .try_lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|k| k.restored.as_raw().len() * 4))
        .unwrap_or(0);
    let framed = FRAMED
        .try_lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref()
                .map(|f| f.source.as_raw().len() * 4 + f.detail.len() * 4)
        })
        .unwrap_or(0);
    kept + framed
}

/// The mask as a small grey picture, white where Restore applies, at most
/// `longest` pixels on its long side - what the window shows.
pub fn mask_preview(framed: &Framed, setting: f32, longest: u32) -> image::GrayImage {
    let (w, h) = framed.source.dimensions();
    let mask = mask_from_detail(&framed.detail, w as usize, h as usize, setting);
    let full = image::GrayImage::from_raw(
        w,
        h,
        mask.par_iter()
            .map(|m| (m.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect(),
    )
    .expect("one byte per pixel");
    let scale = (longest as f32 / w.max(h) as f32).min(1.0);
    if scale >= 1.0 {
        return full;
    }
    image::imageops::resize(
        &full,
        ((w as f32 * scale).round() as u32).max(1),
        ((h as f32 * scale).round() as u32).max(1),
        image::imageops::FilterType::Triangle,
    )
}

/// The mask as a picture, white where Restore applies - for tests and for
/// looking at what the slider does.
#[allow(dead_code)]
pub fn mask_image(image: &Rgb32FImage, setting: f32) -> RgbImage {
    let mask = detail_mask(image, setting);
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let v = (mask[(y * image.width() + x) as usize] * 255.0).round() as u8;
        image::Rgb([v, v, v])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn flat(w: u32, h: u32, v: f32) -> Rgb32FImage {
        Rgb32FImage::from_pixel(w, h, Rgb([v, v, v]))
    }

    /// Bars four pixels wide: the kind of edge a whisker or an eyelid makes.
    fn bars(w: u32, h: u32) -> Rgb32FImage {
        Rgb32FImage::from_fn(w, h, |x, _| {
            let v = if (x / 4) % 2 == 0 { 0.2 } else { 0.6 };
            Rgb([v, v, v])
        })
    }

    #[test]
    fn the_default_is_the_threshold_tuned_on_aks_frames() {
        let (low, high) = thresholds(DEFAULT_MASK);
        assert!((low - 0.012).abs() < 1e-6 && (high - 0.0348).abs() < 1e-6);
    }

    #[test]
    fn smooth_areas_keep_the_original() {
        // Out-of-focus background with grain the blur takes out: nothing to restore.
        let source = Rgb32FImage::from_fn(64, 64, |x, y| {
            let n = (((x * 7919 + y * 104_729) % 97) as f32 / 97.0 - 0.5) * 0.004;
            Rgb([0.3 + n, 0.3 + n, 0.3 + n])
        });
        let restored = flat(64, 64, 0.9);
        let out = blend(&source, &restored, DEFAULT_MASK);
        for (o, s) in out.pixels().zip(source.pixels()) {
            assert!((o[0] - s[0]).abs() < 1e-3, "{} vs {}", o[0], s[0]);
        }
    }

    #[test]
    fn edges_take_the_restored_photo() {
        let source = bars(64, 64);
        let mask = detail_mask(&source, DEFAULT_MASK);
        let middle = mask[32 * 64 + 32];
        assert!(middle > 0.99, "mask at an edge was {middle}");
    }

    #[test]
    fn zero_is_the_whole_photo() {
        let source = flat(16, 16, 0.3);
        let restored = flat(16, 16, 0.8);
        let out = blend(&source, &restored, 0.0);
        assert!(out.pixels().all(|p| (p[0] - 0.8).abs() < 1e-6));
    }

    #[test]
    fn a_tighter_mask_never_reaches_further() {
        let source = Rgb32FImage::from_fn(96, 96, |x, y| {
            let v = 0.3 + 0.15 * ((x as f32 * 0.5).sin() * (y as f32 * 0.3).cos());
            Rgb([v, v, v])
        });
        let loose = detail_mask(&source, 20.0);
        let tight = detail_mask(&source, 90.0);
        assert!(loose.iter().zip(&tight).all(|(l, t)| t <= &(l + 1e-4)));
    }

    /// One test, because both slots are shared by the whole process.
    #[test]
    fn the_window_keeps_one_photo_and_one_model_output() {
        let a = bars(8, 8);
        let mut b = bars(8, 8);
        b.put_pixel(0, 0, Rgb([0.0, 0.0, 0.0]));
        keep(key_of(&a), Arc::new(flat(8, 8, 0.5)));
        assert!(kept(&key_of(&a)).is_some());
        assert!(kept(&key_of(&b)).is_none());

        // The photo decodes once while the window has it; the mask view is
        // small whatever the photo's size.
        let mut loads = 0;
        let first = framed("a.CR3", || {
            loads += 1;
            Ok((bars(3000, 2000), (3000, 2000)))
        })
        .unwrap();
        let again = framed("a.CR3", || {
            loads += 1;
            Ok((bars(3000, 2000), (3000, 2000)))
        })
        .unwrap();
        assert_eq!(loads, 1);
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!(first.key, key_of(&first.source));
        assert_eq!(
            mask_preview(&first, DEFAULT_MASK, 2400).dimensions(),
            (2400, 1600)
        );
        assert_eq!(
            mask_from_detail(&first.detail, 3000, 2000, DEFAULT_MASK),
            detail_mask(&first.source, DEFAULT_MASK)
        );

        forget();
        assert!(kept(&key_of(&a)).is_none());
        assert!(current_framed().is_none());
    }
}
