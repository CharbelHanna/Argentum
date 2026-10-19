//! The Looks tab's own data: the presets that come with the app, and which
//! looks are starred. Ours.
//!
//! The presets that come with the app are RAWmakase's (assets/presets @
//! 7c3c81a, MIT): Lightroom XMP files in five groups, shipped unchanged in
//! resources/presets/rawmakase beside their licence, and read through the
//! app's own XMP import (preset_converter.rs), so they become what an imported
//! XMP becomes. Their six B&W presets are left out until there is a black and
//! white mode to read them: they rely on Convert to Grayscale and the gray
//! mixer, which the import does not map.
//!
//! The starred looks are a list of ids, kept in argentum-processing.json: a
//! built-in preset's id from here, a preset's own id, and a LUT's path.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use super::ag_settings;

/// The groups shipped, in the order RAWmakase lists them, without B&W.
pub const GROUPS: [&str; 5] = ["Color", "Creative", "Curve", "Grain", "Vignetting"];

const FAVOURITES_KEY: &str = "looksFavourites";

#[derive(Serialize, Debug)]
pub struct BuiltinPreset {
    /// `rawmakase:<group>/<file name>`: the same on every machine, so a star stays on it.
    pub id: String,
    pub name: String,
    pub adjustments: Value,
}

#[derive(Serialize, Debug)]
pub struct PresetGroup {
    pub name: String,
    pub presets: Vec<BuiltinPreset>,
}

/// The presets under `dir`, a folder per group, each file read as an imported
/// XMP is. A file that does not read is left out rather than failing the list.
pub fn builtin_presets(dir: &Path) -> Vec<PresetGroup> {
    GROUPS
        .iter()
        .filter_map(|group| {
            let mut files: Vec<_> = std::fs::read_dir(dir.join(group))
                .ok()?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("xmp"))
                })
                .collect();
            files.sort();
            let presets: Vec<BuiltinPreset> = files
                .iter()
                .filter_map(|path| {
                    let text = std::fs::read_to_string(path).ok()?;
                    let preset = crate::preset_converter::convert_xmp_to_preset(&text).ok()?;
                    let stem = path.file_stem()?.to_string_lossy();
                    Some(BuiltinPreset {
                        id: format!("rawmakase:{group}/{stem}"),
                        name: preset.name,
                        adjustments: preset.adjustments,
                    })
                })
                .collect();
            (!presets.is_empty()).then(|| PresetGroup {
                name: (*group).to_string(),
                presets,
            })
        })
        .collect()
}

/// The starred looks' ids, none until one is starred.
pub fn favourites(library: &Path) -> Vec<String> {
    ag_settings::get(library, FAVOURITES_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// Keep `ids` as the starred looks.
pub fn save_favourites(library: &Path, ids: &[String]) -> Result<(), String> {
    ag_settings::set(library, FAVOURITES_KEY, Value::from(ids.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/presets/rawmakase")
    }

    #[test]
    fn every_shipped_preset_reads_into_its_group() {
        let groups = builtin_presets(&shipped());
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, GROUPS);
        let counts: Vec<usize> = groups.iter().map(|g| g.presets.len()).collect();
        assert_eq!(counts, [5, 2, 7, 3, 3]);
        for preset in groups.iter().flat_map(|g| &g.presets) {
            assert!(!preset.name.is_empty(), "{} has no name", preset.id);
            assert!(
                preset
                    .adjustments
                    .as_object()
                    .is_some_and(|a| !a.is_empty()),
                "{} changes nothing",
                preset.id
            );
        }
        assert!(
            groups[0]
                .presets
                .iter()
                .any(|p| p.id == "rawmakase:Color/Warm")
        );
    }

    #[test]
    fn starred_looks_are_what_is_read_back() {
        let dir = std::env::temp_dir().join("argentum-looks-favourites");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        assert!(favourites(&dir).is_empty());
        let ids = vec![
            "rawmakase:Color/Warm".to_string(),
            "C:/LUTs/Teal.cube".to_string(),
        ];
        save_favourites(&dir, &ids).expect("write");
        assert_eq!(favourites(&dir), ids);
    }
}
