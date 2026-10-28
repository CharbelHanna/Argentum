//! Which program unpacks a RAW's pixels: rawler, or FlashRaw inside rawler's
//! image (the engine's `flashraw`). Ours.
//!
//! The app's default is a preference, `rawDecoder` in argentum-processing.json
//! (Settings > Processing > RAW decoder), rawler unless changed. A photo can
//! name its own in its edit, `rawDecoder` in its sidecar's adjustments
//! (Edit > Profile). Either way the picture is the same: FlashRaw is only used
//! on the formats where it was measured to give rawler's image, faster, and
//! rawler decodes everything else.
//!
//! THE CHOICE IS THE FILE'S
//!
//! The app decodes a file once and its virtual copies share that decode, so
//! the file's own sidecar decides, not a copy's.
//!
//! NOT IN THE DECODE FINGERPRINT
//!
//! `cache_utils::DecodeFingerprint` holds the settings that change a decode's
//! pixels, so a photo decoded under another value is decoded again. This one
//! changes none: FlashRaw is only used where it gives rawler's image. A cached
//! decode is right under either decoder, and keying on it would only decode
//! photos again for nothing.
//!
//! WHY THE INTERFACE TELLS THIS MODULE, AND DOES NOT SAVE THE SIDECAR
//!
//! A changed choice has to be in force before the photo is decoded again, a
//! moment after the change. Their debounced save writes the sidecar 300 ms
//! later, and a second writer racing it is what made a chosen camera profile
//! revert (argentum/CameraProfile.tsx). So the sidecar keeps one writer, and
//! the interface hands the choice here too (`remember`), where it wins for the
//! rest of the session.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use rawler::decoders::{Decoder, RawDecodeParams};
use rawler::rawimage::RawImage;
use rawler::rawsource::RawSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RawDecoder {
    Rawler,
    FlashRaw,
}

/// The key of a decoder choice: the app's default in our preferences, and a
/// photo's own in its edit's adjustments.
const KEY: &str = "rawDecoder";

/// The app's default, as saved (mods/ag_settings.rs).
const SETTING: super::ag_settings::Setting<RawDecoder> = super::ag_settings::Setting::new(KEY);

static FLASHRAW_BY_DEFAULT: AtomicBool = AtomicBool::new(false);

/// Choices made this session, by file; `None` is "the default" chosen.
static CHOSEN: Mutex<BTreeMap<String, Option<RawDecoder>>> = Mutex::new(BTreeMap::new());

/// The app's default.
pub fn default_decoder() -> RawDecoder {
    if FLASHRAW_BY_DEFAULT.load(Ordering::Relaxed) {
        RawDecoder::FlashRaw
    } else {
        RawDecoder::Rawler
    }
}

/// Read the default at startup. Missing or unreadable means rawler.
pub fn load(library: &Path) {
    let saved = SETTING.load(library);
    FLASHRAW_BY_DEFAULT.store(saved == Some(RawDecoder::FlashRaw), Ordering::Relaxed);
}

/// Write the default, and use it from the next decode.
pub fn save(library: &Path, decoder: RawDecoder) -> Result<(), String> {
    FLASHRAW_BY_DEFAULT.store(decoder == RawDecoder::FlashRaw, Ordering::Relaxed);
    SETTING.save(library, &decoder)
}

/// A photo's choice, from the interface, in force from the next decode.
pub fn remember(photo_path: &str, decoder: Option<RawDecoder>) {
    CHOSEN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(file_of(photo_path), decoder);
}

/// The decoder this photo's file is decoded with.
pub fn for_photo(photo_path: Option<&str>) -> RawDecoder {
    let Some(file) = photo_path.map(file_of) else {
        return default_decoder();
    };
    let chosen = CHOSEN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&file)
        .copied();
    chosen
        .unwrap_or_else(|| saved_choice(&file))
        .unwrap_or_else(default_decoder)
}

/// Decode with the photo's decoder: FlashRaw's samples when it is chosen and
/// handles the file, rawler's full decode otherwise.
pub fn raw_image(
    source: &RawSource,
    decoder: &dyn Decoder,
    file_bytes: &[u8],
    photo_path: Option<&str>,
) -> rawler::Result<RawImage> {
    if for_photo(photo_path) == RawDecoder::FlashRaw
        && let Some(raw) = argentum_engine::flashraw::decode(file_bytes, source, decoder)
    {
        return Ok(raw);
    }
    decoder.raw_image(source, &RawDecodeParams::default(), false)
}

/// Would FlashRaw decode this photo's file, when chosen?
pub fn flashraw_handles(photo_path: &str) -> Result<bool, String> {
    let file = file_of(photo_path);
    let bytes = crate::file_management::read_file_mapped(Path::new(&file))
        .map_err(|e| format!("{file}: {e}"))?;
    let source = RawSource::new_from_slice(&bytes);
    Ok(rawler::get_decoder(&source)
        .is_ok_and(|d| argentum_engine::flashraw::handles(&bytes, &source, d.as_ref())))
}

/// The file a photo path names: a virtual copy's is its original's.
fn file_of(photo_path: &str) -> String {
    crate::file_management::parse_virtual_path(photo_path)
        .0
        .to_string_lossy()
        .into_owned()
}

/// The choice saved in the file's own sidecar.
fn saved_choice(file: &str) -> Option<RawDecoder> {
    let (_, sidecar) = crate::file_management::parse_virtual_path(file);
    choice_in(&std::fs::read_to_string(sidecar).ok()?)
}

/// `adjustments.rawDecoder` in a sidecar's text. Most have none, and some
/// carry megabytes of masks, so the text is only parsed when the key is in it.
fn choice_in(sidecar: &str) -> Option<RawDecoder> {
    if !sidecar.contains("\"rawDecoder\"") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(sidecar).ok()?;
    serde_json::from_value(value.get("adjustments")?.get(KEY)?.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("argentum-raw-decoder-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn a_sidecar_names_its_choice_in_its_adjustments() {
        let at = |v: &str| {
            format!(
                r#"{{"version":1,"rating":0,"adjustments":{{"exposure":0.3,"rawDecoder":{v}}}}}"#
            )
        };
        assert_eq!(choice_in(&at(r#""flashraw""#)), Some(RawDecoder::FlashRaw));
        assert_eq!(choice_in(&at(r#""rawler""#)), Some(RawDecoder::Rawler));
        assert_eq!(choice_in(&at("null")), None);
        assert_eq!(choice_in(&at(r#""dcraw""#)), None);
        assert_eq!(
            choice_in(r#"{"version":1,"adjustments":{"exposure":0.3}}"#),
            None
        );
        // Not in the adjustments, so not a choice.
        assert_eq!(
            choice_in(r#"{"rawDecoder":"flashraw","adjustments":{}}"#),
            None
        );
        assert_eq!(choice_in("not json \"rawDecoder\""), None);
    }

    /// One test, because the default is the app's one global.
    #[test]
    fn the_default_survives_a_restart_and_starts_as_rawler() {
        let dir = scratch("default");
        load(&dir);
        assert_eq!(default_decoder(), RawDecoder::Rawler, "nothing saved yet");

        save(&dir, RawDecoder::FlashRaw).expect("save");
        FLASHRAW_BY_DEFAULT.store(false, Ordering::Relaxed); // as if restarted
        load(&dir);
        assert_eq!(default_decoder(), RawDecoder::FlashRaw);

        std::fs::write(dir.join("argentum-processing.json"), "not json").expect("write");
        load(&dir);
        assert_eq!(
            default_decoder(),
            RawDecoder::Rawler,
            "a corrupt file decides nothing"
        );
    }

    #[test]
    fn this_session_wins_over_the_sidecar_and_copies_follow_their_file() {
        let dir = scratch("photo");
        let file = dir.join("IMG_0001.CR3");
        std::fs::write(
            dir.join("IMG_0001.CR3.agdata"),
            r#"{"version":1,"adjustments":{"rawDecoder":"flashraw"}}"#,
        )
        .expect("sidecar");
        let file = file.to_string_lossy().into_owned();
        let copy = format!("{file}?vc=a1b2c3");

        assert_eq!(
            for_photo(Some(&file)),
            RawDecoder::FlashRaw,
            "the sidecar's"
        );
        assert_eq!(
            for_photo(Some(&copy)),
            RawDecoder::FlashRaw,
            "a copy is its file's"
        );

        remember(&copy, Some(RawDecoder::Rawler));
        assert_eq!(
            for_photo(Some(&file)),
            RawDecoder::Rawler,
            "chosen this session"
        );

        remember(&file, None);
        assert_eq!(
            for_photo(Some(&file)),
            default_decoder(),
            "the default, chosen"
        );
    }

    /// FlashRaw's image against rawler's on real files: every field and every
    /// sample equal, and the app's develop of both compared in dE2000. With
    /// the decode times.
    ///
    /// ```bash
    /// AG_LOOK_SAMPLES=<folder of RAWs> (or AG_RAWS="a.CR3;b.NEF") \
    ///   cargo test --lib mods::raw_decoder::tests::flashraw_gives_rawlers_image -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs RAW files: AG_LOOK_SAMPLES or AG_RAWS"]
    fn flashraw_gives_rawlers_image() {
        let files = sample_files();
        if files.is_empty() {
            eprintln!("SKIPPED: no RAWs in AG_LOOK_SAMPLES or AG_RAWS");
            return;
        }
        let mut failures = Vec::new();
        println!("file | decoder | rawler ms | FlashRaw ms | samples | develop dE2000 mean / max");
        for path in files {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let bytes = std::fs::read(&path).expect("read");
            let source = RawSource::new_from_slice(&bytes);
            let Ok(decoder) = rawler::get_decoder(&source) else {
                println!("{name} | not a RAW rawler opens");
                continue;
            };
            let decoder = decoder.as_ref();
            let rawler_ms = || {
                let t = std::time::Instant::now();
                let raw = decoder.raw_image(&source, &RawDecodeParams::default(), false);
                (raw, t.elapsed().as_secs_f64() * 1e3)
            };
            let (theirs, _) = rawler_ms();
            let Ok(theirs) = theirs else {
                println!("{name} | rawler cannot decode it");
                continue;
            };
            let t = std::time::Instant::now();
            let Some(ours) = argentum_engine::flashraw::decode(&bytes, &source, decoder) else {
                println!("{name} | rawler");
                continue;
            };
            let flashraw_ms = t.elapsed().as_secs_f64() * 1e3;
            let (_, rawler_ms) = rawler_ms();

            if fields(&theirs) != fields(&ours) {
                failures.push(format!("{name}: metadata differs"));
            }
            let same = match (&theirs.data, &ours.data) {
                (
                    rawler::rawimage::RawImageData::Integer(a),
                    rawler::rawimage::RawImageData::Integer(b),
                ) => a == b,
                _ => false,
            };
            if !same {
                failures.push(format!("{name}: samples differ"));
            }
            let (mean, max) = develop_difference(&bytes, &path);
            println!(
                "{name} | FlashRaw | {rawler_ms:.0} | {flashraw_ms:.0} | {} | {mean:.3} / {max:.3}",
                if same { "same" } else { "DIFFER" }
            );
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// The app's develop of a file under each default, in dE2000.
    fn develop_difference(bytes: &[u8], path: &Path) -> (f64, f64) {
        let develop = |flashraw: bool| {
            FLASHRAW_BY_DEFAULT.store(flashraw, Ordering::Relaxed);
            crate::raw_processing::develop_raw_image(bytes, false, 2.5, "auto".into(), None, None)
                .expect("develop")
                .to_rgb32f()
        };
        let before = FLASHRAW_BY_DEFAULT.load(Ordering::Relaxed);
        let (theirs, ours) = (develop(false), develop(true));
        FLASHRAW_BY_DEFAULT.store(before, Ordering::Relaxed);
        assert_eq!(theirs.dimensions(), ours.dimensions(), "{}", path.display());
        let (mut sum, mut max, mut n) = (0.0, 0.0f64, 0usize);
        for (a, b) in theirs.pixels().zip(ours.pixels()).step_by(7) {
            let d = argentum_engine::colour::delta_e2000(
                argentum_engine::colour::lab(a.0),
                argentum_engine::colour::lab(b.0),
            );
            sum += d;
            max = max.max(d);
            n += 1;
        }
        (sum / n.max(1) as f64, max)
    }

    /// Every field of rawler's image but the samples, as text.
    fn fields(r: &RawImage) -> Vec<String> {
        let mut matrices: Vec<String> = r
            .color_matrix
            .iter()
            .map(|(k, v)| format!("{k:?}={v:?}"))
            .collect();
        matrices.sort();
        vec![
            format!("{}x{}x{} bps {}", r.width, r.height, r.cpp, r.bps),
            format!("{:?} {:?} {:?}", r.wb_coeffs, r.whitelevel, r.blacklevel),
            format!("{:?} {}", r.xyz_to_cam, matrices.join(" ")),
            format!("{:?} {:?}", r.photometric, r.orientation),
            format!("{:?} {:?} {:?}", r.active_area, r.crop_area, r.blackareas),
            format!(
                "{} {} {} {} {}",
                r.make,
                r.model,
                r.clean_make,
                r.clean_model,
                r.dng_tags.len()
            ),
        ]
    }

    /// The RAWs to compare: a folder (`AG_LOOK_SAMPLES`, as CI downloads the
    /// golden photos) or a list (`AG_RAWS`, separated by `;`).
    fn sample_files() -> Vec<std::path::PathBuf> {
        let mut files: Vec<std::path::PathBuf> = std::env::var("AG_LOOK_SAMPLES")
            .ok()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        files.extend(
            std::env::var("AG_RAWS")
                .unwrap_or_default()
                .split(';')
                .filter(|s| !s.trim().is_empty())
                .map(std::path::PathBuf::from),
        );
        files.sort();
        files
    }
}
