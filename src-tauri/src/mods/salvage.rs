//! A stored file that no longer reads. Ours.
//!
//! WHY THIS EXISTS
//!
//! Settings, sidecars and our preferences file were read with
//! `from_str(..).unwrap_or_default()`. One value of an unexpected type (a key
//! whose type changed between versions, a hand edit, a file cut short by a
//! crash) and the whole file read as defaults, and the next save wrote those
//! defaults over it: every setting gone at the next start, or a photo's whole
//! edit at its next rating. Nothing said so.
//!
//! So a file that does not read is copied to `<name>.bak` before anything can
//! write over it, the failure is logged, and what can be read is kept: every
//! top-level key the type accepts. Only the keys it refuses fall back.

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Where the copy of an unreadable `path` goes: `settings.json` -> `settings.json.bak`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Copy `path`, which did not read, beside itself, and log why. A copy that
/// already holds the same bytes is left alone, so a file read over and over
/// (a sidecar in a grid) is copied and logged once.
pub fn keep_copy(path: &Path, error: &dyn Display) {
    let backup = backup_path(path);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::error!(
                "{} could not be read ({error}), nor copied: {e}",
                path.display()
            );
            return;
        }
    };
    // An empty file has nothing to keep.
    if bytes.is_empty() || fs::read(&backup).is_ok_and(|kept| kept == bytes) {
        return;
    }
    match fs::write(&backup, &bytes) {
        Ok(()) => log::error!(
            "{} could not be read ({error}); a copy is kept at {}",
            path.display(),
            backup.display()
        ),
        Err(e) => log::error!(
            "{} could not be read ({error}), and no copy could be kept: {e}",
            path.display()
        ),
    }
}

/// `stored`'s top-level keys laid over `base` one at a time, each kept only if
/// `T` still reads with it; the value, and the keys it refused. `None` when
/// `base` itself does not read as `T`.
pub fn readable_keys<T: DeserializeOwned>(
    base: Map<String, Value>,
    stored: Map<String, Value>,
) -> Option<(T, Vec<String>)> {
    let mut kept = base;
    let mut refused = Vec::new();
    for (key, value) in stored {
        let previous = kept.insert(key.clone(), value);
        if serde_json::from_value::<T>(Value::Object(kept.clone())).is_err() {
            match previous {
                Some(previous) => kept.insert(key.clone(), previous),
                None => kept.remove(&key),
            };
            refused.push(key);
        }
    }
    let value = serde_json::from_value(Value::Object(kept)).ok()?;
    Some((value, refused))
}

/// `content`, the text of `path`, read as `T`. When it does not read, a copy is
/// kept (`keep_copy`) and every key `T` accepts is read over `base()`; a file
/// that is not a JSON object at all reads as `T::default()`.
pub fn read_or_keep<T: DeserializeOwned + Default>(
    path: &Path,
    content: &str,
    base: impl FnOnce() -> Map<String, Value>,
) -> T {
    let error = match serde_json::from_str(content) {
        Ok(value) => return value,
        Err(error) => error,
    };
    keep_copy(path, &error);
    let Ok(Value::Object(stored)) = serde_json::from_str::<Value>(content) else {
        return T::default();
    };
    match readable_keys(base(), stored) {
        Some((value, refused)) => {
            if !refused.is_empty() {
                log::warn!(
                    "{}: read every key but {}",
                    path.display(),
                    refused.join(", ")
                );
            }
            value
        }
        None => T::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argentum-salvage-{label}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[derive(Deserialize, Default, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Prefs {
        #[serde(default)]
        theme: Option<String>,
        #[serde(default)]
        zoom: u32,
        #[serde(default)]
        folders: Vec<String>,
    }

    #[test]
    fn the_keys_that_read_are_kept_and_the_others_named() {
        let stored = serde_json::json!({ "theme": "dark", "zoom": "big", "folders": ["a"] });
        let Value::Object(stored) = stored else {
            unreachable!()
        };
        let (prefs, refused) = readable_keys::<Prefs>(Map::new(), stored).expect("base reads");
        assert_eq!(
            prefs,
            Prefs {
                theme: Some("dark".into()),
                zoom: 0,
                folders: vec!["a".into()],
            }
        );
        assert_eq!(refused, vec!["zoom".to_string()]);
    }

    #[test]
    fn an_unreadable_file_is_copied_once_and_read_as_far_as_it_goes() {
        let dir = scratch("copy");
        let path = dir.join("settings.json");
        let text = r#"{ "theme": "dark", "zoom": -3 }"#;
        fs::write(&path, text).unwrap();

        let prefs: Prefs = read_or_keep(&path, text, Map::new);
        assert_eq!(prefs.theme.as_deref(), Some("dark"));
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), text);

        // Read again: the copy is the same bytes, so it is not written again.
        let before = fs::metadata(backup_path(&path))
            .unwrap()
            .modified()
            .unwrap();
        let _: Prefs = read_or_keep(&path, text, Map::new);
        let after = fs::metadata(backup_path(&path))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn a_file_cut_short_reads_as_defaults_and_is_still_kept() {
        let dir = scratch("short");
        let path = dir.join("settings.json");
        let text = r#"{ "theme": "da"#;
        fs::write(&path, text).unwrap();

        let prefs: Prefs = read_or_keep(&path, text, Map::new);
        assert_eq!(prefs, Prefs::default());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), text);
    }

    /// The real settings: a key whose type changed loses only itself.
    #[test]
    fn settings_with_one_unreadable_key_keep_all_the_others() {
        let dir = scratch("settings");
        let path = dir.join("settings.json");
        let text = r#"{ "theme": "grey", "rootFolders": ["D:/Photos"], "exportPresets": 5 }"#;
        fs::write(&path, text).unwrap();

        let settings: crate::app_settings::AppSettings = read_or_keep(&path, text, Map::new);
        assert_eq!(settings.theme.as_deref(), Some("grey"));
        assert_eq!(settings.root_folders, vec!["D:/Photos".to_string()]);
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), text);
    }

    /// The real sidecar: snapshots in a shape this version cannot read must not
    /// take the edit, the rating or the tags with them.
    #[test]
    fn a_sidecar_with_unreadable_snapshots_keeps_its_edit() {
        let dir = scratch("sidecar");
        let path = dir.join("IMG_0001.CR3.agdata");
        let text = r#"{ "version": 1, "rating": 4, "adjustments": { "exposure": 0.7 },
            "tags": ["sea"], "agSnapshots": { "not": "a list" } }"#;
        fs::write(&path, text).unwrap();

        let meta = crate::exif_processing::load_sidecar(&path);
        assert_eq!(meta.rating, 4);
        assert_eq!(meta.adjustments["exposure"], 0.7);
        assert_eq!(meta.tags, Some(vec!["sea".to_string()]));
        assert!(meta.ag_snapshots.is_none());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), text);
    }

    #[test]
    fn a_file_that_reads_leaves_no_copy() {
        let dir = scratch("fine");
        let path = dir.join("settings.json");
        let text = r#"{ "theme": "dark" }"#;
        fs::write(&path, text).unwrap();

        let _: Prefs = read_or_keep(&path, text, Map::new);
        assert!(!backup_path(&path).exists());
    }
}
