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
//! original repository. It is written beside its final name and moved into
//! place only once its SHA-256 matches. Not yet taken from it: resuming a
//! half-finished file, and the lock between two running copies of the app.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rm_selection::manifest::{ModelFile, all_files, total_bytes};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

/// Where the five files live.
pub fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join("rawmakase-selection"))
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

fn sha256_of(path: &Path) -> Result<String, String> {
    let mut reader = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Fetch whatever is missing or wrong. `progress` hears the bytes done of the
/// bytes to fetch.
pub async fn download(dir: &Path, progress: impl Fn(u64, u64)) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let wanted: Vec<ModelFile> = all_files()
        .into_iter()
        .filter(|f| sha256_of(&dir.join(f.name)).ok().as_deref() != Some(f.sha256))
        .collect();
    let total: u64 = wanted.iter().map(|f| f.size_bytes).sum();
    let mut done = 0u64;
    for file in wanted {
        let mut last_error = String::new();
        let mut fetched = false;
        for url in [file.url, file.fallback] {
            match fetch(dir, &file, url, done, total, &progress).await {
                Ok(()) => {
                    fetched = true;
                    break;
                }
                Err(e) => last_error = format!("{url}: {e}"),
            }
        }
        if !fetched {
            return Err(format!("Could not download {}: {last_error}", file.name));
        }
        done += file.size_bytes;
        progress(done, total);
    }
    *VERIFIED.lock().map_err(|e| e.to_string())? = None;
    verify(dir)
}

/// One file from one source, into a sibling `.part`, moved into place once its
/// checksum matches.
async fn fetch(
    dir: &Path,
    file: &ModelFile,
    url: &str,
    before: u64,
    total: u64,
    progress: &impl Fn(u64, u64),
) -> Result<(), String> {
    let part = dir.join(format!("{}.part", file.name));
    let mut response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut got = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        got += chunk.len() as u64;
        if got > file.size_bytes {
            drop(out);
            let _ = std::fs::remove_file(&part);
            return Err("larger than expected".to_string());
        }
        hasher.update(&chunk);
        out.write_all(&chunk).map_err(|e| e.to_string())?;
        progress(before + got, total);
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if got != file.size_bytes || hex(&hasher.finalize()) != file.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err("checksum does not match".to_string());
    }
    std::fs::rename(&part, dir.join(file.name)).map_err(|e| e.to_string())
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
