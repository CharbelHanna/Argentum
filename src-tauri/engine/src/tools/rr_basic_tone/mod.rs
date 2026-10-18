//! Contrast, highlights, shadows, whites and blacks: RapidRAW's, as the app
//! runs them.
//!
//! Each works on luminance and keeps colour: a pixel is scaled by how far
//! its luminance moved. Highlights, shadows, whites and blacks move the light
//! a pixel sits in rather than its texture - the guided filter
//! (`guided.rs`) says which is which - and put the texture back on top, so
//! lifting the shadows does not flatten them. Contrast is an S-curve around
//! middle grey, leaving values above white alone.
//!
//! Copied from the app @ c786c146: `apply_tonal_adjustments`,
//! `apply_highlights_adjustment` and `shape_detail` in
//! `src-tauri/src/shaders/shader.wgsl`, in the order its `main` runs them,
//! and the sliders' scale (`SCALES` in `src-tauri/src/image_processing.rs`).
//! The settings are the app's slider values, -100 to 100.
//!
//! WHAT CHANGED IN THE COPY
//!
//! The app measures detail on the picture as decoded, before exposure and
//! white balance, once per photo. Here it is measured on this tool's input
//! and kept for as long as that input is the same. Exposure makes no
//! difference to it (in log luminance it shifts every fitted line alike);
//! white balance changes it only where colours meet.

mod guided;

use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::colour::luma;
use crate::{Context, Error, Field, Image, Result, Rgb, Section, Stage, Tool};
pub use guided::{Coefficients, GF_LUMA_FLOOR, log_luma};

/// The app's slider value that means 1 in the shader, per slider (`SCALES`).
const CONTRAST_SCALE: f32 = 100.0;
const HIGHLIGHTS_SCALE: f32 = 120.0;
const SHADOWS_SCALE: f32 = 120.0;
const WHITES_SCALE: f32 = 40.0;
const BLACKS_SCALE: f32 = 40.0;

/// How sharply detail is let back in: `GF_REINJECT_SIGMA`.
const GF_REINJECT_SIGMA: f32 = 0.8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
        }
    }
}

impl crate::Settings for Settings {
    const VERSION: u32 = 1;
    const FIELDS: &'static [Field] = &[
        Field::new("contrast").range(-100.0, 100.0).step(1.0),
        Field::new("highlights").range(-100.0, 100.0).step(1.0),
        Field::new("shadows").range(-100.0, 100.0).step(1.0),
        Field::new("whites").range(-100.0, 100.0).step(1.0),
        Field::new("blacks").range(-100.0, 100.0).step(1.0),
    ];
}

/// The settings as the shader takes them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Amounts {
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
}

impl From<&Settings> for Amounts {
    fn from(s: &Settings) -> Self {
        Self {
            contrast: s.contrast / CONTRAST_SCALE,
            highlights: s.highlights / HIGHLIGHTS_SCALE,
            shadows: s.shadows / SHADOWS_SCALE,
            whites: s.whites / WHITES_SCALE,
            blacks: s.blacks / BLACKS_SCALE,
        }
    }
}

impl Amounts {
    /// Whether any slider that reads detail is off zero.
    fn needs_detail(&self) -> bool {
        self.shadows != 0.0 || self.blacks != 0.0 || self.highlights != 0.0 || self.whites != 0.0
    }
}

#[derive(Default)]
pub struct RrBasicTone {
    /// The last input's guided-filter coefficients, by input key.
    coefficients: Mutex<Option<(u64, Arc<Coefficients>)>>,
}

impl Tool for RrBasicTone {
    const ID: &'static str = "rr_basic_tone";
    const STAGE: Stage = Stage::SceneLinear;
    const SECTION: Section = Section::Basic;
    type Settings = Settings;

    fn process(&self, settings: &Settings, input: &Image, cx: &Context) -> Result<Option<Image>> {
        let rgb = input.as_rgb().ok_or_else(|| Error::Tool {
            tool: Self::ID,
            message: format!("was handed {}", input.describe()),
        })?;
        if *settings == Settings::default() {
            return Ok(None);
        }
        let amounts = Amounts::from(settings);
        let detail = amounts
            .needs_detail()
            .then(|| self.coefficients(cx.input_key, rgb));
        let w = rgb.width;
        let pixels = rgb
            .pixels
            .par_iter()
            .enumerate()
            .map(|(i, &p)| {
                let (local, mid) = match &detail {
                    Some(c) => c.detail(i % w, i / w, log_luma(p)),
                    None => (0.0, 0.0),
                };
                basic_tone(p, local, mid, &amounts)
            })
            .collect();
        Ok(Some(Image::Rgb(rgb.with_pixels(pixels))))
    }
}

impl RrBasicTone {
    fn coefficients(&self, key: u64, rgb: &Rgb) -> Arc<Coefficients> {
        let mut memo = self.coefficients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, c)) = &*memo
            && *k == key
        {
            return c.clone();
        }
        let c = Arc::new(Coefficients::of(rgb));
        *memo = Some((key, c.clone()));
        c
    }
}

/// One pixel through the tool: the shader's `apply_tonal_adjustments` then
/// `apply_highlights_adjustment`, given the pixel's local and mid-scale log
/// detail.
pub fn basic_tone(color: [f32; 3], local: f32, mid: f32, a: &Amounts) -> [f32; 3] {
    let toned = tonal_adjustments(color, local, mid, a.contrast, a.shadows, a.whites, a.blacks);
    highlights_adjustment(toned, local, mid, a.highlights)
}

fn shape_detail(d: f32, sigma: f32) -> f32 {
    let x = d / sigma;
    d / (1.0 + x * x)
}

/// WGSL's `smoothstep`.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// WGSL's `mix`.
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    c.map(|v| v * k)
}

/// `apply_tonal_adjustments`: whites, then shadows and blacks, then contrast.
fn tonal_adjustments(
    color: [f32; 3],
    local_log_detail: f32,
    mid_log_detail: f32,
    con: f32,
    sh: f32,
    wh: f32,
    bl: f32,
) -> [f32; 3] {
    let mut rgb = color;

    if wh != 0.0 {
        let w_luma = luma(rgb.map(|v| v.max(0.0))).max(0.0001);
        let w_base = (w_luma * (-(local_log_detail + mid_log_detail).clamp(-3.0, 3.0)).exp2())
            .max(0.0001);
        let t_w = w_base.powf(0.4545);
        let w_pivot = 0.45;
        let u = (t_w - w_pivot).max(0.0);

        let mut t_new = t_w;
        if wh > 0.0 {
            t_new = t_w + wh * 0.25 * (u * u) / (1.0 + u * u);
        } else if u > 0.0 {
            t_new = w_pivot + u / (1.0 + (-wh) * 0.3 * u);
        }
        let w_ramp = smoothstep(0.25, 0.9, t_w);
        t_new = mix(t_w, t_new, w_ramp);

        let w_amount = (t_new - t_w).abs() * w_ramp;
        rgb = scale(rgb, t_new.powf(2.2) / w_base);

        let w_log_d = local_log_detail * 0.4545;
        let w_safe_detail = (w_log_d / (1.0 + w_log_d.abs() * 0.4)).exp2();
        let w_corr = w_safe_detail.powf(1.0 + w_amount * 1.0) / w_safe_detail;
        rgb = scale(rgb, w_corr.powf(2.2));
        rgb = scale(
            rgb,
            (shape_detail(mid_log_detail, GF_REINJECT_SIGMA) * w_amount * 4.0).exp2(),
        );
    }

    let pixel_luma = luma(rgb.map(|v| v.max(0.0)));
    let safe_pixel_luma = pixel_luma.max(0.0001);

    if sh != 0.0 || bl != 0.0 {
        let t_pixel = safe_pixel_luma.powf(0.4545);
        let t_detail = (local_log_detail * 0.4545).exp2();
        let t_blurred = t_pixel / t_detail.max(0.0001);

        let shadow_lift = sh * t_pixel * (1.0 - t_pixel).max(0.0).powf(4.5);
        let black_lift = bl * t_pixel * (1.0 - t_pixel).max(0.0).powf(12.0);
        let lift_amount = (shadow_lift + black_lift).max(0.0);

        let t_pixel_curved = (t_pixel + shadow_lift + black_lift).max(0.0);

        let shadow_pivot = 0.2;
        let stretch_focus = 1.0 - smoothstep(0.1, 0.85, t_pixel);
        let detail_focus = 1.0 - smoothstep(0.05, 0.55, t_pixel);
        let stretch_factor = 1.0 + (lift_amount * stretch_focus * 4.4);
        let contrasted_t = shadow_pivot + (t_pixel_curved - shadow_pivot) * stretch_factor;

        let final_t = contrasted_t.max(0.0);
        let curved_luma = final_t.powf(2.2);

        let luma_ratio = curved_luma / safe_pixel_luma;
        rgb = scale(rgb, luma_ratio);

        let log_d = t_detail.max(0.0001).log2();
        let safe_detail = (log_d / (1.0 + log_d.abs() * 0.4)).exp2();

        let noise_protection = smoothstep(0.0, 0.1, t_blurred);

        let detail_amp = 1.0 + (lift_amount * detail_focus * 1.5 * noise_protection);

        let enhanced_detail = safe_detail.powf(detail_amp);
        let detail_correction = enhanced_detail / safe_detail;

        let linear_correction = detail_correction.powf(2.2);
        rgb = scale(rgb, linear_correction);

        let lift_structure = lift_amount * detail_focus * noise_protection * 6.0;
        rgb = scale(
            rgb,
            (shape_detail(mid_log_detail, GF_REINJECT_SIGMA) * lift_structure).exp2(),
        );

        if luma_ratio > 1.0 {
            let recovered_luma = luma(rgb);
            let boost_amount = ((luma_ratio - 1.0) * 0.05).clamp(0.0, 0.12);
            rgb = rgb.map(|v| mix(v, recovered_luma, boost_amount));
        }
    }

    if con != 0.0 {
        let g = 2.2;
        let strength = 2.0f32.powf(con * 1.25);
        rgb = rgb.map(|v| {
            let safe = v.max(0.0);
            let perceptual = safe.powf(1.0 / g).clamp(0.0, 1.0);
            let curved = if perceptual < 0.5 {
                0.5 * (2.0 * perceptual).powf(strength)
            } else {
                1.0 - 0.5 * (2.0 * (1.0 - perceptual)).powf(strength)
            };
            let adjusted = curved.powf(g);
            let mix_factor = smoothstep(1.0, 1.01, safe);
            mix(adjusted, v, mix_factor)
        });
    }
    rgb
}

/// `apply_highlights_adjustment`: the light above a pivot compressed or
/// raised, the detail put back.
fn highlights_adjustment(
    color_in: [f32; 3],
    local_log_detail: f32,
    mid_log_detail: f32,
    highlights_adj: f32,
) -> [f32; 3] {
    if highlights_adj.abs() < 0.001 {
        return color_in;
    }

    let pixel_luma = luma(color_in.map(|v| v.max(0.0)));
    if pixel_luma < 0.0001 {
        return color_in;
    }

    const L_PIVOT: f32 = 0.06;
    if pixel_luma <= L_PIVOT {
        return color_in;
    }

    let luma_base = pixel_luma * (-local_log_detail).exp2();
    let detail_ratio = pixel_luma / luma_base.max(1e-4);

    let safe_detail = detail_ratio.clamp(0.65, 1.55);

    let log_detail = safe_detail.log2();
    let soft_log_detail = log_detail / (1.0 + log_detail.abs() * 0.40);

    let target_luma = if highlights_adj < 0.0 {
        let k = -highlights_adj;
        let delta_base = (luma_base - L_PIVOT).max(0.0);

        let compression_strength = k * 2.2;
        let compressed_delta =
            delta_base / (1.0 + compression_strength * (delta_base / (1.0 + delta_base * 0.35)));
        let target_base = L_PIVOT + compressed_delta;

        let neg_soft_detail = local_log_detail / (1.0 + local_log_detail.abs() * 0.80);
        let restoration_gain = 1.0 + k * 0.3;
        let structure_gain = shape_detail(mid_log_detail, GF_REINJECT_SIGMA) * k * 0.6;
        let recovered_detail = (neg_soft_detail * restoration_gain + structure_gain).exp2();

        let recovered_target = target_base * recovered_detail;

        let blend = smoothstep(L_PIVOT, L_PIVOT + 0.22, pixel_luma);
        mix(pixel_luma, recovered_target, blend)
    } else {
        let boost = highlights_adj * 0.70;
        let delta_base = (luma_base - L_PIVOT).max(0.0);
        let boosted_delta = delta_base * (1.0 + boost / (1.0 + delta_base * 0.25));
        let target_base = L_PIVOT + boosted_delta;

        let detail_boost = (soft_log_detail * (1.0 + highlights_adj * 0.20)).exp2();
        let boosted_target = target_base * detail_boost;

        let blend = smoothstep(L_PIVOT, L_PIVOT + 0.22, pixel_luma);
        mix(pixel_luma, boosted_target, blend)
    };

    let luma_ratio = target_luma / pixel_luma;
    let mut final_color = scale(color_in, luma_ratio);

    if highlights_adj < 0.0 && pixel_luma > 1.0 {
        let blowout = smoothstep(1.0, 3.5, pixel_luma) * (-highlights_adj) * 0.40;
        final_color = final_color.map(|v| mix(v, target_luma, blowout));
    }

    final_color
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amounts(f: impl FnOnce(&mut Settings)) -> Amounts {
        let mut s = Settings::default();
        f(&mut s);
        Amounts::from(&s)
    }

    #[test]
    fn each_slider_moves_the_light_its_way() {
        let grey = |v: f32, a: &Amounts| luma(basic_tone([v; 3], 0.0, 0.0, a));
        let shadows = amounts(|s| s.shadows = 60.0);
        assert!(grey(0.02, &shadows) > 0.02);
        let highlights = amounts(|s| s.highlights = -60.0);
        assert!(grey(0.8, &highlights) < 0.8);
        let whites = amounts(|s| s.whites = 50.0);
        assert!(grey(0.7, &whites) > 0.7);
        let blacks = amounts(|s| s.blacks = -50.0);
        assert!(grey(0.005, &blacks) < 0.005);
        let contrast = amounts(|s| s.contrast = 50.0);
        assert!(grey(0.05, &contrast) < 0.05 && grey(0.6, &contrast) > 0.6);
    }

    #[test]
    fn contrast_leaves_light_above_white_alone() {
        let a = amounts(|s| s.contrast = 80.0);
        assert_eq!(basic_tone([2.0, 3.0, 4.0], 0.0, 0.0, &a), [2.0, 3.0, 4.0]);
    }
}
