//! The selection models' files: where they live, whether they are all there,
//! and fetching the ones that are not.
//!
//! They live in the app's model folder under `rawmakase-selection/`, all five
//! in one folder because RAWmakase's loader reads them from one
//! (rm_selection `Subject::load_with`). A copy already on the disk can be
//! hard-linked there, and then nothing is fetched.
//!
//! Fetching follows RAWmakase's installer (src/app/subject_mask/models.rs @
//! 7c3c81a): nothing downloads unless asked, and the size is given first. Every
//! file comes from a pinned commit: RAWmakase's mirror first, then the
//! original repository. The download itself is `model_download`'s, shared with
//! Settings > General > AI Models, where these five are the "Selection masks"
//! entry (`model_catalog::SELECTION`): each file is written beside its final
//! name and moved into place only once its SHA-256 matches. Not yet taken from
//! RAWmakase: resuming a half-finished file, and the lock between two running
//! copies of the app.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rm_selection::manifest::{ModelFile, all_files, total_bytes};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::model_catalog::{File, SELECTION};
use super::model_download::{Progress, fetch_missing, sha256_of};

/// Their folder inside the models folder.
pub const FOLDER: &str = "rawmakase-selection";

/// Where the five files live.
pub fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(FOLDER))
}

/// The five, as the catalogue and the downloader describe a file.
pub fn files() -> Vec<File> {
    all_files()
        .into_iter()
        .map(|f| File {
            name: f.name,
            url: f.url,
            fallback: Some(f.fallback),
            sha256: f.sha256,
            bytes: f.size_bytes,
        })
        .collect()
}

/// Whether the models are there, and how much is left to fetch.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installed: bool,
    pub missing_bytes: u64,
    pub total_bytes: u64,
}

/// By size alone: cheap enough to ask before every selection.
pub fn status(dir: &Path) -> Status {
    let missing_bytes = all_files()
        .iter()
        .filter(|f| !has_size(dir, f))
        .map(|f| f.size_bytes)
        .sum();
    Status {
        installed: missing_bytes == 0,
        missing_bytes,
        total_bytes: total_bytes(),
    }
}

fn has_size(dir: &Path, file: &ModelFile) -> bool {
    std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.is_file() && m.len() == file.size_bytes)
}

/// The folder whose files were last found to match their checksums, so each is
/// read through once per run, not once per selection.
static VERIFIED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Every file there and every checksum right.
pub fn verify(dir: &Path) -> Result<(), String> {
    if VERIFIED.lock().map_err(|e| e.to_string())?.as_deref() == Some(dir) {
        return Ok(());
    }
    for file in all_files() {
        let path = dir.join(file.name);
        if !has_size(dir, &file) {
            return Err(format!("{} is missing", file.name));
        }
        if sha256_of(&path)? != file.sha256 {
            return Err(format!("{} does not match its checksum", file.name));
        }
    }
    *VERIFIED.lock().map_err(|e| e.to_string())? = Some(dir.to_path_buf());
    Ok(())
}

/// Fetch whatever is missing or wrong, then check all five.
pub async fn download(dir: &Path, progress: impl Fn(&Progress)) -> Result<(), String> {
    fetch_missing(dir, SELECTION, &files(), progress).await?;
    *VERIFIED.lock().map_err(|e| e.to_string())? = None;
    verify(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ag-selection-models-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An empty folder lacks everything; the size is RAWmakase's 271 MB.
    #[test]
    fn an_empty_folder_needs_every_file() {
        let dir = scratch("empty");
        let status = status(&dir);
        assert!(!status.installed);
        assert_eq!(status.missing_bytes, total_bytes());
        assert_eq!(status.total_bytes, 184_115_050 + 86_559_030);
        assert!(verify(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file of the right size with the wrong bytes is not taken as installed.
    /// The smallest file is enough to show it, without writing 271 MB.
    #[test]
    fn the_right_size_with_the_wrong_bytes_fails_the_check() {
        let dir = scratch("wrong");
        let first = all_files()[0];
        let f = std::fs::File::create(dir.join(first.name)).unwrap();
        f.set_len(first.size_bytes).unwrap();
        drop(f);
        assert_eq!(status(&dir).missing_bytes, total_bytes() - first.size_bytes);
        let err = verify(&dir).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
