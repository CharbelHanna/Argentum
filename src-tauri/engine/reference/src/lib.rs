//! Today's app code, compiled outside the app.
//!
//! The engine's tools are copies of modules the app still runs. The parity
//! tests have to compare each copy with the original on the same input, and
//! building the app to do that would defeat the point of a crate that tests
//! on its own. So the original files are compiled here as they are, byte for
//! byte, straight out of `src-tauri/src/` - nothing is copied into this crate.
//!
//! The few things those files reach for in the rest of the app are stood in
//! for below, by name only: a GPU context with a device and a queue, and two
//! settings functions that the tests never call. When one of those files
//! changes what it imports, this crate stops compiling, which is the right
//! moment to look at whether the engine's copy has fallen behind.
//!
//! Test-only. Nothing in the engine depends on it.

// Someone else's files, compiled for their behaviour and not linted here.
#![allow(warnings)]

#[path = "../../../src/mods/ag_settings.rs"]
pub mod ag_settings;
#[path = "../../../src/mods/highlights.rs"]
pub mod highlights;
#[path = "../../../src/mods/sharpen.rs"]
pub mod sharpen;
#[path = "../../../src/mods/sharpen_gpu.rs"]
pub mod sharpen_gpu;

/// `crate::image_processing::GpuContext`, the two fields `sharpen_gpu.rs` reads.
pub mod image_processing {
    use std::sync::Arc;

    pub struct GpuContext {
        pub device: Arc<wgpu::Device>,
        pub queue: Arc<wgpu::Queue>,
    }
}

/// `crate::app_settings`, as `sharpen::retire_presharpening_once` names it.
/// That function needs the running app and is never called here.
pub mod app_settings {
    pub struct AppSettings {
        pub raw_preprocessing_sharpening: Option<f32>,
    }

    pub fn load_settings(_app: tauri::AppHandle) -> Result<AppSettings, String> {
        Err("the app's settings are not part of the reference build".into())
    }

    pub fn save_settings(_settings: AppSettings, _app: tauri::AppHandle) -> Result<(), String> {
        Err("the app's settings are not part of the reference build".into())
    }
}
