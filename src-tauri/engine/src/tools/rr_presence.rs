//! Vibrance and saturation: RapidRAW's, as the app runs them.
//!
//! Both scale each pixel's distance from its own luminance. Saturation does
//! it evenly. Vibrance raises muted colours more than vivid ones and holds
//! back on skin tones; turned down, it takes the most from the most
//! saturated.
//!
//! Copied from the app @ c786c146: `apply_creative_color`, `rgb_to_hsv` and
//! `smoothstep`'s use in `src-tauri/src/shaders/shader.wgsl`, and the
//! sliders' scale (`SCALES` in `src-tauri/src/image_processing.rs`). The
//! settings are the app's slider values, -100 to 100.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::colour::{self, luma};
use crate::{Context, Error, Field, Image, Result, Section, Stage, Tool};

/// The app's slider value that means a factor of 1 (`SCALES`).
const SLIDER_SCALE: f32 = 100.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub vibrance: f32,
    pub saturation: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            vibrance: 0.0,
            saturation: 0.0,
        }
    }
}

impl crate::Settings for Settings {
    const VERSION: u32 = 1;
    const FIELDS: &'static [Field] = &[
        Field::new("vibrance").range(-100.0, 100.0).step(1.0),
        Field::new("saturation").range(-100.0, 100.0).step(1.0),
    ];
}

#[derive(Default)]
pub struct RrPresence;

impl Tool for RrPresence {
    const ID: &'static str = "rr_presence";
    const STAGE: Stage = Stage::SceneLinear;
    const SECTION: Section = Section::Basic;
    type Settings = Settings;

    fn process(&self, settings: &Settings, input: &Image, _cx: &Context) -> Result<Option<Image>> {
        let rgb = input.as_rgb().ok_or_else(|| Error::Tool {
            tool: Self::ID,
            message: format!("was handed {}", input.describe()),
        })?;
        let sat = settings.saturation / SLIDER_SCALE;
        let vib = settings.vibrance / SLIDER_SCALE;
        if sat == 0.0 && vib == 0.0 {
            return Ok(None);
        }
        let pixels = rgb
            .pixels
            .par_iter()
            .map(|&p| creative_color(p, sat, vib))
            .collect();
        Ok(Some(Image::Rgb(rgb.with_pixels(pixels))))
    }
}

/// `apply_creative_color`, for a pixel: `sat` and `vib` as the shader takes
/// them, the slider value over 100.
pub fn creative_color(color: [f32; 3], sat: f32, vib: f32) -> [f32; 3] {
    let positive = color.map(|v| v.max(0.0));
    let luma = luma(positive);

    let srgb = positive.map(colour::srgb_encode_extended);
    let [hue, current_sat, _] = rgb_to_hsv(srgb);

    let mut vib_factor = 0.0;
    if vib != 0.0 {
        if vib > 0.0 {
            let sat_weight = (1.0 - current_sat).max(0.0).powf(1.25);
            let skin_center = 25.0;
            let hue_dist = (hue - skin_center)
                .abs()
                .min(360.0 - (hue - skin_center).abs());
            let is_skin = 1.0 - smoothstep(12.0, 38.0, hue_dist);
            let skin_dampener = mix(1.0, 0.50, is_skin);

            vib_factor = vib * sat_weight * skin_dampener * 1.5;
        } else {
            let desat_weight = smoothstep(0.05, 0.75, current_sat);
            vib_factor = vib * desat_weight;
        }
    }

    let sat_scale = (1.0 + sat).max(0.0);
    let vib_scale = (1.0 + vib_factor).max(0.0);
    let final_mult = sat_scale * vib_scale;

    color.map(|c| mix(luma, c, final_mult).max(0.0))
}

/// `rgb_to_hsv`: hue in degrees, saturation and value 0..1.
fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = c;
    let c_max = r.max(g.max(b));
    let c_min = r.min(g.min(b));
    let delta = c_max - c_min;
    let mut h = 0.0;
    if delta > 0.0 {
        h = if c_max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if c_max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
    }
    if h < 0.0 {
        h += 360.0;
    }
    let s = if c_max > 0.0 { delta / c_max } else { 0.0 };
    [h, s, c_max]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grey_stays_grey() {
        let out = creative_color([0.3; 3], 0.5, 0.5);
        assert!(out.iter().all(|v| (v - 0.3).abs() < 1e-6), "{out:?}");
    }

    #[test]
    fn saturation_moves_away_from_luminance_and_back() {
        let c = [0.5, 0.3, 0.1];
        let more = creative_color(c, 0.5, 0.0);
        let less = creative_color(c, -1.0, 0.0);
        assert!(more[0] - more[2] > c[0] - c[2]);
        let l = luma(c);
        assert!(less.iter().all(|v| (v - l).abs() < 1e-6), "{less:?}");
    }

    #[test]
    fn vibrance_lifts_a_muted_colour_more_than_a_vivid_one() {
        let spread = |c: [f32; 3]| c[2] - c[0];
        let muted = [0.30, 0.32, 0.36];
        let vivid = [0.05, 0.15, 0.6];
        let gain = |c| spread(creative_color(c, 0.0, 0.5)) / spread(c);
        assert!(gain(muted) > gain(vivid));
    }
}
