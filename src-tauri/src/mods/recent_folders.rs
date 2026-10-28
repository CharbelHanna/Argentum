//! The folders opened most recently, newest first, for the Recent list in the
//! Modern layout's folder column. Ours.
//!
//! Kept in argentum-processing.json through `ag_settings`, beside the editor
//! layout. Only paths are kept; what a folder holds is read from disk when it
//! is opened, as for any other folder.

use std::path::Path;

use super::ag_settings::Setting;

const RECENT: Setting<Vec<String>> = Setting::new("recentFolders");

/// How many the list keeps.
pub const MAX_RECENT: usize = 6;

/// The list as saved, newest first, whether or not each folder is there now.
pub fn stored(library: &Path) -> Vec<String> {
    RECENT.load(library).unwrap_or_default()
}

/// Put `folder` at the top, dropping any older entry for the same folder and
/// whatever falls past `MAX_RECENT`. Returns the list as saved.
pub fn add(library: &Path, folder: &str) -> Result<Vec<String>, String> {
    let mut list = stored(library);
    list.retain(|f| !same_folder(f, folder));
    list.insert(0, folder.to_string());
    list.truncate(MAX_RECENT);
    RECENT.save(library, &list)?;
    Ok(list)
}

/// The list once the folder `old` is renamed to `new`: it and every folder
/// inside it keep their place under the new name, as `pathAfterFolderRename`
/// (src/argentum/fileOps.ts) moves the library's own paths. A folder listed
/// under both names (opened again under the new one first) is listed once, in
/// its newer place. Returns the list as saved.
pub fn renamed(library: &Path, old: &str, new: &str) -> Result<Vec<String>, String> {
    let list = stored(library);
    let mut after: Vec<String> = Vec::with_capacity(list.len());
    for folder in &list {
        let folder = match folder.strip_prefix(old) {
            Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
                format!("{new}{rest}")
            }
            _ => folder.clone(),
        };
        if !after.iter().any(|kept| same_folder(kept, &folder)) {
            after.push(folder);
        }
    }
    if after != list {
        RECENT.save(library, &after)?;
    }
    Ok(after)
}

/// The same folder written two ways: a trailing separator, and on Windows
/// the case of the letters and the direction of the slashes, do not count.
fn same_folder(a: &str, b: &str) -> bool {
    let trim = |s: &str| s.trim_end_matches(['/', '\\']).to_string();
    if cfg!(windows) {
        trim(a)
            .replace('/', "\\")
            .eq_ignore_ascii_case(&trim(b).replace('/', "\\"))
    } else {
        trim(a) == trim(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argentum-recent-folders-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn empty_until_a_folder_is_opened() {
        assert!(stored(&scratch("empty")).is_empty());
    }

    #[test]
    fn newest_first_and_each_folder_once() {
        let dir = scratch("order");
        add(&dir, "/photos/a").expect("add");
        add(&dir, "/photos/b").expect("add");
        let list = add(&dir, "/photos/a/").expect("add");
        assert_eq!(list, vec!["/photos/a/", "/photos/b"]);
        assert_eq!(stored(&dir), list);
    }

    #[test]
    fn a_renamed_folder_and_those_inside_it_keep_their_place() {
        let dir = scratch("renamed");
        for folder in [
            "/photos/x",
            "/photos/trip2",
            "/photos/trip/day 1",
            "/photos/trip",
        ] {
            add(&dir, folder).expect("add");
        }
        let list = renamed(&dir, "/photos/trip", "/photos/rome").expect("rename");
        assert_eq!(
            list,
            vec![
                "/photos/rome",
                "/photos/rome/day 1",
                "/photos/trip2",
                "/photos/x"
            ]
        );
        assert_eq!(stored(&dir), list);
    }

    #[test]
    fn a_folder_opened_again_under_its_new_name_is_listed_once() {
        let dir = scratch("renamed-twice");
        add(&dir, "/photos/trip").expect("add");
        add(&dir, "/photos/rome").expect("add");
        let list = renamed(&dir, "/photos/trip", "/photos/rome").expect("rename");
        assert_eq!(list, vec!["/photos/rome"]);
    }

    #[test]
    fn keeps_only_the_most_recent() {
        let dir = scratch("cap");
        for i in 0..MAX_RECENT + 3 {
            add(&dir, &format!("/photos/{i}")).expect("add");
        }
        let list = stored(&dir);
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], format!("/photos/{}", MAX_RECENT + 2));
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_match_whatever_their_case_and_slashes() {
        let dir = scratch("windows");
        add(&dir, r"E:\2024\Muir Woods").expect("add");
        let list = add(&dir, "e:/2024/muir woods").expect("add");
        assert_eq!(list, vec!["e:/2024/muir woods"]);
    }

    #[test]
    fn saving_leaves_the_other_preferences_alone() {
        let dir = scratch("neighbours");
        let neighbour = Setting::<serde_json::Value>::new("editorLayout");
        neighbour
            .save(&dir, &serde_json::Value::from("modern"))
            .expect("set");
        add(&dir, "/photos/a").expect("add");
        assert_eq!(
            neighbour.load(&dir),
            Some(serde_json::Value::from("modern"))
        );
    }
}
