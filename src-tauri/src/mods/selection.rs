//! One-click Subject, Sky and Background masks, from RAWmakase.
//!
//! The models and the code that runs them are RAWmakase's (`src-tauri/rm_selection`,
//! crates/rawmakase-inference @ 7c3c81a, MIT): DETR panoptic finds the photo's
//! people, animals and sky, and SAM 2.1 draws their outlines. Argentum's part
//! is here: the picture they are given, the answer in the form our AI masks
//! already store, and when the models are let go.
//!
//! The picture is the one their mask models read (lib.rs
//! `get_cached_full_warped_image`: the RAW developed, its geometry warp
//! applied, before crop and rotation). It is scaled to a 1280 px long edge,
//! which is what RAWmakase gives its models (src/app/subject_mask/worker.rs:30).
//! The answer comes back at that size and is scaled to the working image's,
//! the frame a stored AI mask is drawn from (mask_generation.rs
//! `generate_ai_bitmap_from_full_mask`). It is stored as their Foreground or
//! Sky mask stores its own (ai_processing.rs `AiForegroundMaskParameters`), so
//! it renders through their code unchanged.
//!
//! One analysis is kept, for the last photo and geometry: Subject, Sky and
//! Background on the same photo answer from it without running a model again.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, GrayImage, ImageFormat};
use rm_selection::{Analysis, LoadOptions, Session, Subject};
use serde::{Deserialize, Serialize};

/// RAWmakase's working size for these models.
const LONG_EDGE: u32 = 1280;

/// Which part of the photo to select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Subject,
    Sky,
    Background,
}

/// What the mask stores: the fields of their `AiForegroundMaskParameters`, and
/// whether anything was found at all.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub mask_data_base64: String,
    pub rotation: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub orientation_steps: u8,
    pub found: bool,
}

/// The photo's frame as the mask stores it, passed through unchanged.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub flip_horizontal: bool,
    #[serde(default)]
    pub flip_vertical: bool,
    #[serde(default)]
    pub orientation_steps: u8,
}

struct Loaded {
    dir: PathBuf,
    session: Session,
    analysis: Option<(u64, Analysis)>,
    used: Instant,
}

static LOADED: Mutex<Option<Loaded>> = Mutex::new(None);

/// Which photo and geometry a picture is: the analysis is reused while it stays the same.
pub fn picture_key(path: &str, js_adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    crate::cache_utils::calculate_geometry_hash(js_adjustments).hash(&mut hasher);
    hasher.finish()
}

/// Select `kind` in `picture` (the warped working image) with the models in `dir`.
pub fn select(
    dir: &Path,
    picture: &Arc<DynamicImage>,
    key: u64,
    kind: Kind,
    frame: Frame,
) -> Result<Selection, String> {
    super::selection_models::verify(dir)?;
    let (width, height) = picture.dimensions();
    let small = picture
        .resize(LONG_EDGE, LONG_EDGE, FilterType::Triangle)
        .to_rgb8();
    let input = rm_selection::RgbImage {
        width: small.width() as usize,
        height: small.height() as usize,
        data: small.into_raw(),
    };

    let mut slot = LOADED.lock().map_err(|e| e.to_string())?;
    if slot.as_ref().is_none_or(|l| l.dir != dir) {
        let options = LoadOptions {
            // The runtime the app's other models already use (lib.rs sets it).
            runtime_library: std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from),
            ..LoadOptions::default()
        };
        let session = Subject::load_with(dir, &options).map_err(|e| format!("{e:?}"))?;
        *slot = Some(Loaded {
            dir: dir.to_path_buf(),
            session,
            analysis: None,
            used: Instant::now(),
        });
    }
    let loaded = slot.as_mut().ok_or("the selection models did not load")?;
    loaded.used = Instant::now();
    if loaded.analysis.as_ref().map(|(k, _)| *k) != Some(key) {
        let cancel = AtomicBool::new(false);
        let analysis = loaded
            .session
            .analyze(&input, &cancel)
            .map_err(|e| format!("{e:?}"))?;
        loaded.analysis = Some((key, analysis));
    }
    let (_, analysis) = loaded.analysis.as_ref().ok_or("no analysis")?;
    let coverage = match kind {
        Kind::Subject | Kind::Background => analysis.subject(),
        Kind::Sky => analysis.sky(),
    }
    .map_err(|e| format!("{e:?}"))?;
    drop(slot);

    let found = coverage.data.iter().any(|v| *v > 0);
    let mut mask =
        GrayImage::from_raw(coverage.width as u32, coverage.height as u32, coverage.data)
            .ok_or("the selection came back the wrong size")?;
    if kind == Kind::Background {
        image::imageops::invert(&mut mask);
    }
    let full = image::imageops::resize(&mask, width, height, FilterType::Triangle);
    Ok(Selection {
        mask_data_base64: png_data_url(&full)?,
        rotation: frame.rotation,
        flip_horizontal: frame.flip_horizontal,
        flip_vertical: frame.flip_vertical,
        orientation_steps: frame.orientation_steps,
        found,
    })
}

/// As their AI masks store theirs (ai_commands.rs `encode_to_base64_png`).
fn png_data_url(mask: &GrayImage) -> Result<String, String> {
    let mut bytes = Cursor::new(Vec::new());
    mask.write_to(&mut bytes, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(bytes.get_ref())
    ))
}

/// Let the models go once nothing has asked for a while (memory.rs's watcher).
pub fn unload_if_idle(after: Duration) -> bool {
    let Ok(mut slot) = LOADED.try_lock() else {
        return false;
    };
    if slot.as_ref().is_some_and(|l| l.used.elapsed() > after) {
        *slot = None;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real models on a real photo: set AG_SELECTION_MODELS to the folder of
    /// the five files, AG_SELECTION_PHOTO to a RAW with a person in it, and
    /// ORT_DYLIB_PATH to onnxruntime.dll, then run with --ignored.
    #[test]
    #[ignore = "needs the models, a runtime and a photo"]
    fn a_person_is_found_and_background_is_the_rest() {
        let dir = PathBuf::from(std::env::var("AG_SELECTION_MODELS").unwrap());
        let photo = std::env::var("AG_SELECTION_PHOTO").unwrap();
        let bytes = std::fs::read(&photo).unwrap();
        let picture = Arc::new(crate::raw_processing::extract_embedded_preview(&bytes).unwrap());
        let frame = Frame {
            rotation: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            orientation_steps: 0,
        };
        let started = Instant::now();
        let subject = select(&dir, &picture, 1, Kind::Subject, frame).unwrap();
        let first = started.elapsed();
        let started = Instant::now();
        let background = select(&dir, &picture, 1, Kind::Background, frame).unwrap();
        let again = started.elapsed();
        eprintln!("subject {first:?}, background from the same analysis {again:?}");
        assert!(subject.found && background.found);
        assert!(
            subject
                .mask_data_base64
                .starts_with("data:image/png;base64,")
        );
        assert!(again < first, "the second selection reused the analysis");
    }

    /// Another geometry is another picture; the same one is the same.
    #[test]
    fn the_analysis_is_reused_only_for_the_same_photo_and_geometry() {
        let flat = serde_json::json!({});
        let tilted = serde_json::json!({ "transformRotate": 5.0 });
        assert_eq!(picture_key("a.CR3", &flat), picture_key("a.CR3", &flat));
        assert_ne!(picture_key("a.CR3", &flat), picture_key("b.CR3", &flat));
        assert_ne!(picture_key("a.CR3", &flat), picture_key("a.CR3", &tilted));
    }
}
