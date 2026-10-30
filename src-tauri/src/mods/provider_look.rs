//! A provider look on a photo: a LUT made for that photo alone. Ours.
//!
//! The file sits beside its photo as `<photo>.<camera>.<look>.lut.png`
//! (`file_ops::look_file_for`). The edit holds no path, so a photo that moves,
//! or an edit pasted onto another photo, finds that photo's own file:
//!
//! - `lutPath` is exactly [`MARKER`]. The marker is the switch: anything that
//!   writes a real `lutPath` turns the look off, with no code of its own.
//! - `agProviderLook` ([`KEY`]) names the camera and the look, and keeps what
//!   the look replaced, so taking it off restores the rendering
//!   (src/argentum/looks/providerLook.ts).
//!
//! Every load of an edit's LUT goes through [`lut_of_edit`]. A path from an edit
//! is refused if it could point anywhere (`parse_lut_file`); this one is built
//! from the photo and two checked ids, so a photo on a network share or in a
//! folder with `..` in its name still finds its file.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::State;

use crate::AppState;
use crate::file_management::parse_virtual_path;
use crate::lut_processing::{self, Lut, get_or_load_lut};
use crate::mods::file_ops;
use crate::mods::lut_cache::LutCache;
use crate::mods::raw_tone::ToneCurvePoint;

/// The `lutPath` of an edit with a provider look on.
pub const MARKER: &str = "provider-look";

/// The key in an edit that says which provider look is on.
pub const KEY: &str = "agProviderLook";

/// A provider look as an edit holds it, under `agProviderLook`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ProviderLook {
    pub camera: String,
    pub look: String,
    #[serde(default)]
    pub before: RenderingBefore,
}

/// The rendering a provider look replaced on its photo.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct RenderingBefore {
    pub raw_tone_rendering: Option<String>,
    pub raw_tone_curve: Option<Vec<ToneCurvePoint>>,
    pub raw_tone_curve_mode: Option<String>,
}

/// The file of the provider look `js` has on `photo`; None for an edit with
/// none, or one whose ids could not be part of a file name.
pub fn look_file(js: &Value, photo: &str) -> Option<PathBuf> {
    if js["lutPath"].as_str() != Some(MARKER) {
        return None;
    }
    let look = ProviderLook::deserialize(&js[KEY]).ok()?;
    file_ops::look_file_for(&parse_virtual_path(photo).0, &look.camera, &look.look)
}

/// The LUT an edit asks for: a provider look's file beside the photo, else
/// `lutPath`.
pub fn lut_of_edit(state: &State<AppState>, js: &Value, photo: &str) -> Option<Arc<Lut>> {
    let named = js["lutPath"].as_str()?;
    if named != MARKER {
        return get_or_load_lut(state, named).ok();
    }
    load_look_file(&state.lut_cache, &look_file(js, photo)?)
}

/// An enlargement of `photo` is another file, so the look stays on it with a
/// copy of the look file under its name. It holds `rawToneRendering` as it opens
/// (super_resolution.rs `carry_raw_look`), so that is what taking the look off
/// gives it.
pub fn carry_to_enlargement(adjustments: &mut Value, photo: &Path, enlarged: &Path) {
    let name_for = |path: &Path| look_file(adjustments, &path.to_string_lossy());
    let (Some(from), Some(to)) = (name_for(photo), name_for(enlarged)) else {
        return;
    };
    if let Err(e) = std::fs::copy(&from, &to) {
        log::warn!("Super resolution: the look file was not carried over: {e}");
    }
    adjustments[KEY]["before"] = json!({
        "rawToneRendering": adjustments["rawToneRendering"],
        "rawToneCurve": null,
        "rawToneCurveMode": null,
    });
}

/// Settings pasted onto a photo, which may be a photo the interface does not
/// have open: a LUT in them replaces the look on the photo, which gives back
/// the rendering it replaced. Call before the pasted keys go in, so a rendering
/// among them still wins.
pub fn give_way_to_paste(edit: &mut Value, pasted: &Value) {
    let brings_a_lut = pasted
        .get("lutPath")
        .is_some_and(|path| path.as_str() != Some(MARKER));
    if !brings_a_lut || edit["lutPath"].as_str() != Some(MARKER) {
        return;
    }
    let before = edit[KEY]["before"].take();
    for key in ["rawToneRendering", "rawToneCurve", "rawToneCurveMode"] {
        edit[key] = before[key].clone();
    }
    edit[KEY] = Value::Null;
}

/// The LUT in `file`, through the cache; None once the file is gone, whatever
/// the cache still holds.
fn load_look_file(cache: &Mutex<LutCache>, file: &Path) -> Option<Arc<Lut>> {
    if !file.is_file() {
        return None;
    }
    let key = file.to_string_lossy();
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(lut) = cache.get(&key) {
        return Some(lut);
    }
    let lut = Arc::new(lut_processing::parse_hald(image::open(file).ok()?).ok()?);
    cache.insert(key.into_owned(), Arc::clone(&lut));
    Some(lut)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::dng_standard::gpu::{centre, device, image_of, render_with_lut};
    use crate::white_balance::WhiteBalance;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argentum-provider-look-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn edit(camera: &str, look: &str) -> Value {
        json!({ "lutPath": MARKER, KEY: { "camera": camera, "look": look } })
    }

    /// A 16-step LUT that visibly changes a picture: its channels turned round.
    fn write_look_file(path: &Path) {
        let mut png = image::RgbImage::new(64, 64);
        for (i, pixel) in png.pixels_mut().enumerate() {
            let step = |n: usize| ((n % 16) * 17) as u8;
            let (r, g, b) = (step(i), step(i / 16), step(i / 256));
            *pixel = image::Rgb([g, b, r]);
        }
        png.save(path).expect("look file");
    }

    #[test]
    fn a_marked_edit_names_the_file_beside_its_photo() {
        assert_eq!(
            look_file(&edit("cam-1", "look2"), "/photos/IMG.CR3"),
            Some(PathBuf::from("/photos/IMG.CR3.cam-1.look2.lut.png"))
        );
    }

    /// Same photo, same white balance: a virtual copy shares the photo's file.
    #[test]
    fn a_virtual_copy_names_its_photos_file() {
        assert_eq!(
            look_file(&edit("cam1", "abc"), "/photos/IMG.CR3?vc=a1b2c3"),
            Some(PathBuf::from("/photos/IMG.CR3.cam1.abc.lut.png"))
        );
    }

    /// The name that does the look decides it: the file is the photo's, and
    /// an edit file from elsewhere cannot aim at any other.
    #[test]
    fn ids_that_could_not_be_a_look_files_name_name_nothing() {
        for (camera, look) in [
            ("..", "abc"),
            ("../..", "abc"),
            ("cam1", "a/b"),
            ("cam1", r"a\b"),
            ("Cam1", "abc"),
            ("", "abc"),
            ("cam1", ""),
            ("cam1", "-abc"),
            ("cam1", "a.b"),
            ("cam1", "abcdefghijklmnopq"),
        ] {
            assert_eq!(
                look_file(&edit(camera, look), "/photos/IMG.CR3"),
                None,
                "{camera} {look}"
            );
        }
    }

    #[test]
    fn only_the_exact_marker_counts() {
        let mut js = edit("cam1", "abc");
        for other in [json!("film.cube"), json!("Provider-Look"), Value::Null] {
            js["lutPath"] = other;
            assert_eq!(look_file(&js, "/photos/IMG.CR3"), None);
        }
        assert_eq!(
            look_file(&json!({ "lutPath": MARKER }), "/photos/IMG.CR3"),
            None
        );
    }

    /// The renderer takes either path to the same pixels: a marked edit gets
    /// the LUT a path to the file gets.
    #[test]
    fn a_marked_edit_gets_the_lut_its_file_holds() {
        let dir = scratch("same");
        let photo = dir.join("IMG.CR3");
        let file = dir.join("IMG.CR3.cam1.abc.lut.png");
        write_look_file(&file);

        let cache = Mutex::new(LutCache::default());
        let found = look_file(&edit("cam1", "abc"), &photo.to_string_lossy()).expect("a file");
        let marked = load_look_file(&cache, &found).expect("a LUT");
        let named = lut_processing::parse_lut_file(&file.to_string_lossy()).expect("a LUT");
        assert_eq!(marked.size, named.size);
        assert_eq!(marked.data, named.data);
    }

    /// `parse_lut_file`'s refusals are for paths taken from an edit, and this
    /// one is not.
    #[test]
    fn a_look_file_loads_where_an_edits_path_would_be_refused() {
        let dir = scratch("dots").join("trip..2026");
        std::fs::create_dir_all(&dir).expect("folder");
        let file = dir.join("IMG.CR3.cam1.abc.lut.png");
        write_look_file(&file);
        assert!(lut_processing::parse_lut_file(&file.to_string_lossy()).is_err());
        let cache = Mutex::new(LutCache::default());
        assert!(load_look_file(&cache, &file).is_some());
    }

    /// The strip says the file is missing, so the render must not still show it.
    #[test]
    fn a_look_file_gone_gives_no_lut_even_once_cached() {
        let dir = scratch("gone");
        let file = dir.join("IMG.CR3.cam1.abc.lut.png");
        write_look_file(&file);
        let cache = Mutex::new(LutCache::default());
        assert!(load_look_file(&cache, &file).is_some());
        std::fs::remove_file(&file).expect("delete");
        assert!(load_look_file(&cache, &file).is_none());
    }

    #[test]
    fn a_broken_look_file_gives_no_lut() {
        let dir = scratch("broken");
        let file = dir.join("IMG.CR3.cam1.abc.lut.png");
        std::fs::write(&file, "not a picture").expect("file");
        let cache = Mutex::new(LutCache::default());
        assert!(load_look_file(&cache, &file).is_none());
    }

    fn with_look_on() -> Value {
        let mut js = edit("cam1", "abc");
        js[KEY]["before"] = json!({
            "rawToneRendering": "baseCurve",
            "rawToneCurve": [{ "x": 0.0, "y": 0.0 }, { "x": 255.0, "y": 255.0 }],
            "rawToneCurveMode": "baseCurve",
        });
        js["rawToneRendering"] = json!("linear");
        js
    }

    #[test]
    fn a_pasted_lut_gives_the_photo_its_rendering_back() {
        let mut js = with_look_on();
        give_way_to_paste(&mut js, &json!({ "lutPath": "film.cube" }));
        assert_eq!(js["rawToneRendering"], "baseCurve");
        assert_eq!(js["rawToneCurveMode"], "baseCurve");
        assert_eq!(js["rawToneCurve"].as_array().expect("curve").len(), 2);
        assert_eq!(js[KEY], Value::Null);
    }

    #[test]
    fn a_paste_with_no_lut_leaves_the_look_on() {
        let mut js = with_look_on();
        let before = js.clone();
        give_way_to_paste(&mut js, &json!({ "exposure": 1.0 }));
        give_way_to_paste(&mut js, &json!({ "lutPath": MARKER }));
        assert_eq!(js, before);
    }

    #[test]
    fn a_paste_onto_a_photo_with_no_look_changes_nothing() {
        let mut js = json!({ "lutPath": "old.cube", "rawToneRendering": "default" });
        let before = js.clone();
        give_way_to_paste(&mut js, &json!({ "lutPath": "film.cube" }));
        assert_eq!(js, before);
    }

    #[test]
    fn an_enlargement_keeps_the_look_under_its_own_name() {
        let dir = scratch("enlarge");
        let (photo, enlarged) = (dir.join("IMG.CR3"), dir.join("IMG_2x.tif"));
        write_look_file(&dir.join("IMG.CR3.cam1.abc.lut.png"));

        let mut js = edit("cam1", "abc");
        js[KEY]["before"] = json!({ "rawToneRendering": "dngStandard" });
        js["rawToneRendering"] = json!("default");
        carry_to_enlargement(&mut js, &photo, &enlarged);

        let found = look_file(&js, &enlarged.to_string_lossy()).expect("a file");
        assert!(found.is_file());
        assert_eq!(js[KEY]["before"]["rawToneRendering"], "default");
    }

    #[test]
    fn an_enlargement_of_a_photo_with_no_look_gets_no_file() {
        let dir = scratch("enlarge-none");
        let mut js = json!({ "lutPath": "film.cube", "rawToneRendering": "default" });
        let before = js.clone();
        carry_to_enlargement(&mut js, &dir.join("IMG.CR3"), &dir.join("IMG_2x.tif"));
        assert_eq!(js, before);
        assert_eq!(std::fs::read_dir(&dir).expect("folder").count(), 0);
    }

    /// A load that skips `lut_of_edit` would hand the marker to the file
    /// parser, which finds no such file, and the photo would show no look in
    /// that one place. These are every file that renders an edit.
    #[test]
    fn every_render_finds_its_lut_through_the_edit() {
        for (name, source) in [
            ("lib.rs", include_str!("../lib.rs")),
            (
                "export_processing.rs",
                include_str!("../export_processing.rs"),
            ),
            ("file_management.rs", include_str!("../file_management.rs")),
            ("mods/commands.rs", include_str!("commands.rs")),
        ] {
            assert!(!source.contains("get_or_load_lut"), "{name}");
        }
    }

    /// Through the real shader: `cargo test --lib provider_look -- --ignored`.
    #[test]
    #[ignore = "needs a GPU; run with --ignored"]
    fn a_marked_edit_renders_as_one_naming_the_same_file() {
        const PATCHES: [[f32; 3]; 4] = [
            [0.18, 0.18, 0.18],
            [0.9, 0.9, 0.9],
            [0.6, 0.05, 0.03],
            [0.04, 0.12, 0.7],
        ];
        let dir = scratch("render");
        let photo = dir.join("IMG.CR3");
        let file = dir.join("IMG.CR3.cam1.abc.lut.png");
        write_look_file(&file);

        let ctx = device().expect("no wgpu adapter");
        let render = |js: Value, lut: Arc<Lut>| {
            let out = render_with_lut(
                &ctx,
                &image_of(&PATCHES),
                js,
                WhiteBalance::reference(),
                Some(lut),
            );
            (0..PATCHES.len())
                .map(|i| centre(&out, i))
                .collect::<Vec<_>>()
        };
        let settings = json!({
            "rawToneRendering": "linear",
            "agSharpen": { "capture": false, "amount": 0 },
            "lutIntensity": 100,
            "lutIsSceneReferred": false,
            "agLutDeclaredInput": true,
        });

        let mut named_edit = settings.clone();
        named_edit["lutPath"] = json!(file.to_string_lossy());
        let named = lut_processing::parse_lut_file(&file.to_string_lossy()).expect("a LUT");

        let mut marked_edit = settings.clone();
        marked_edit["lutPath"] = json!(MARKER);
        marked_edit[KEY] = json!({ "camera": "cam1", "look": "abc" });
        let cache = Mutex::new(LutCache::default());
        let found = look_file(&marked_edit, &photo.to_string_lossy()).expect("a file");
        let marked = load_look_file(&cache, &found).expect("a LUT");

        let without = render(settings, Arc::new(named.clone()));
        let want = render(named_edit, Arc::new(named));
        assert_ne!(want, without, "the LUT did nothing, so this proves nothing");
        assert_eq!(render(marked_edit, marked), want);
    }
}
