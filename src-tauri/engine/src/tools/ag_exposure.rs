//! Exposure, in stops. Ours.
//!
//! Every channel of every pixel is multiplied by 2 to the power of the
//! exposure, worked out once in f32, so each pixel takes exactly one
//! multiplication and the result can be checked exactly. The app's
//! `apply_linear_exposure` in `shader.wgsl` is the same multiplication; its
//! slider is in steps of 1.25 stops (`SCALES.exposure`), this one in stops.
//!
//! It runs on linear RGB after the develop and capture sharpening, before
//! white balance, where the app runs it.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{Context, Error, Field, Image, Result, Section, Stage, Tool};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// +1 doubles the light.
    pub exposure_ev: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { exposure_ev: 0.0 }
    }
}

impl crate::Settings for Settings {
    const VERSION: u32 = 1;
    const FIELDS: &'static [Field] = &[Field::new("exposure_ev")
        .range(-5.0, 5.0)
        .step(0.01)
        .unit("EV")];
}

/// What every channel is multiplied by.
pub fn gain(settings: &Settings) -> f32 {
    2.0f32.powf(settings.exposure_ev)
}

#[derive(Default)]
pub struct AgExposure;

impl Tool for AgExposure {
    const ID: &'static str = "ag_exposure";
    const STAGE: Stage = Stage::SceneLinear;
    const SECTION: Section = Section::Basic;
    type Settings = Settings;

    fn process(&self, settings: &Settings, input: &Image, _cx: &Context) -> Result<Option<Image>> {
        let rgb = input.as_rgb().ok_or_else(|| Error::Tool {
            tool: Self::ID,
            message: format!("was handed {}", input.describe()),
        })?;
        let g = gain(settings);
        if g == 1.0 {
            return Ok(None);
        }
        let pixels = rgb
            .pixels
            .par_iter()
            .map(|p| [p[0] * g, p[1] * g, p[2] * g])
            .collect();
        Ok(Some(Image::Rgb(rgb.with_pixels(pixels))))
    }
}
