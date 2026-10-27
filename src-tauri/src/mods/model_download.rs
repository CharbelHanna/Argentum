//! Downloading an AI model's missing files, one download at a time. Ours.
//!
//! Used by Settings > General > AI Models' Download button (`download`) and by
//! the selection masks the first time they are used (`selection_models`).
//!
//! The steps are Shoebox's (src-tauri/src/models.rs @ 3c82753, `download` and
//! `fetch`, with paths.rs `model_file`), which selection_models.rs already
//! followed from RAWmakase (src/app/subject_mask/models.rs @ 7c3c81a): only the
//! files that are missing or wrong are fetched; each is streamed to
//! `<name>.part` beside its final name while its SHA-256 is worked out, and
//! moved into place only once that matches the pinned one. A part file that
//! fails or is stopped is removed. Progress is reported once a MiB, as
//! Shoebox's `Hashed` does, not once a network chunk.
//!
//! The files, addresses and checksums are the catalogue's, which the tests in
//! `model_catalog` hold to the code of each feature: what lands here is what
//! that feature would have fetched itself, so it fetches nothing.
//!
//! ONE AT A TIME
//!
//! Two downloads of the same file would write the same part file. So each
//! download waits for the one before it to finish, and then finds what that one
//! fetched already in place. Clearing leftovers deletes part files, so
//! `model_manager` only clears while it holds `exclusive()`.
//!
//! NEVER THROUGH A LINK
//!
//! A folder that is a link (a junction on Windows) points at files kept
//! somewhere else, such as a copy of the selection models kept outside the app.
//! Nothing is written through one.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::model_catalog::{self, File, Stored};

/// The event a download reports its progress on.
pub const EVENT: &str = "ag-model-download";

/// The error a stopped download ends with.
pub const STOPPED: &str = "Download stopped.";

/// Report at most once per this many bytes.
const REPORT_EVERY: u64 = 1 << 20;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// The catalogue entry being downloaded.
    pub id: String,
    pub done: u64,
    pub total: u64,
}

static RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static CURRENT: Mutex<Option<Progress>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);

/// The download running now, if any: for a settings page opened after it began.
pub fn current() -> Option<Progress> {
    CURRENT.lock().ok().and_then(|c| c.clone())
}

/// Stop the download running now: it removes its part file and fails with
/// `STOPPED`. Nothing happens when none is running.
pub fn stop() {
    if current().is_some() {
        STOP.store(true, Ordering::Relaxed);
    }
}

/// No download starts while the guard is held. None when one is running.
pub fn exclusive() -> Option<tokio::sync::MutexGuard<'static, ()>> {
    RUNNING.try_lock().ok()
}

/// Download catalogue entry `id`'s missing files into the models folder
/// `models`. Returns the bytes fetched: 0 when everything was there.
pub async fn download(
    models: &Path,
    id: &str,
    progress: impl Fn(&Progress),
) -> Result<u64, String> {
    let model = model_catalog::find(id).ok_or_else(|| format!("No AI model called {id}"))?;
    if let Stored::FoldersStartingWith { .. } = model.stored {
        return Err(format!(
            "{} arrives with the feature that needs it, the first time it runs.",
            model.label
        ));
    }
    let dir = match model.stored.folder() {
        Some(folder) => models.join(folder),
        None => models.to_path_buf(),
    };
    fetch_missing(&dir, id, &model.stored.files(), progress).await
}

/// Fetch whatever of `files` is missing from `dir` or does not match its
/// checksum, reporting progress as catalogue entry `id`. Returns the bytes
/// fetched.
pub async fn fetch_missing(
    dir: &Path,
    id: &str,
    files: &[File],
    progress: impl Fn(&Progress),
) -> Result<u64, String> {
    let _running = RUNNING.lock().await;
    STOP.store(false, Ordering::Relaxed);
    if std::fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!(
            "{} is a link to another folder, so Argentum does not download into it",
            dir.display()
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let wanted: Vec<File> = {
        let (dir, files) = (dir.to_path_buf(), files.to_vec());
        tokio::task::spawn_blocking(move || {
            files.into_iter().filter(|f| !in_place(&dir, f)).collect()
        })
        .await
        .map_err(|e| e.to_string())?
    };

    let report = Reporter::new(id, wanted.iter().map(|f| f.bytes).sum(), progress);
    let mut done = 0u64;
    for file in &wanted {
        let mut last_error = String::new();
        let mut fetched = false;
        for url in std::iter::once(file.url).chain(file.fallback) {
            match fetch(dir, file, url, done, &report).await {
                Ok(()) => {
                    fetched = true;
                    break;
                }
                Err(e) if e == STOPPED => return Err(e),
                Err(e) => last_error = format!("{url}: {e}"),
            }
        }
        if !fetched {
            return Err(format!("Could not download {}: {last_error}", file.name));
        }
        done += file.bytes;
        report.at(done);
    }
    Ok(done)
}

/// Here, at its size, with its checksum.
fn in_place(dir: &Path, file: &File) -> bool {
    let path = dir.join(file.name);
    std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() == file.bytes)
        && sha256_of(&path).is_ok_and(|sha| sha == file.sha256)
}

/// Whether a feature's files are all here, and how much is left to fetch.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installed: bool,
    pub missing_bytes: u64,
    pub total_bytes: u64,
}

fn has_size(dir: &Path, file: &File) -> bool {
    std::fs::metadata(dir.join(file.name)).is_ok_and(|m| m.is_file() && m.len() == file.bytes)
}

/// `files` in `dir`, by size alone: cheap enough to ask before every mask.
pub fn status(dir: &Path, files: &[File]) -> Status {
    let missing_bytes = files
        .iter()
        .filter(|f| !has_size(dir, f))
        .map(|f| f.bytes)
        .sum();
    Status {
        installed: missing_bytes == 0,
        missing_bytes,
        total_bytes: files.iter().map(|f| f.bytes).sum(),
    }
}

/// Every one of `files` in `dir`, each with its checksum. `verified` holds the
/// folder last found whole, so each file is read through once per run, not
/// once per mask; set it to None after a download.
pub fn verify(dir: &Path, files: &[File], verified: &Mutex<Option<PathBuf>>) -> Result<(), String> {
    if verified.lock().map_err(|e| e.to_string())?.as_deref() == Some(dir) {
        return Ok(());
    }
    for file in files {
        if !has_size(dir, file) {
            return Err(format!("{} is missing", file.name));
        }
        if sha256_of(&dir.join(file.name))? != file.sha256 {
            return Err(format!("{} does not match its checksum", file.name));
        }
    }
    *verified.lock().map_err(|e| e.to_string())? = Some(dir.to_path_buf());
    Ok(())
}

pub(super) fn sha256_of(path: &Path) -> Result<String, String> {
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
    Ok(hex::encode(hasher.finalize()))
}

/// Keeps `CURRENT` and the caller told, once a MiB; `CURRENT` is emptied
/// when the download ends, however it ends.
struct Reporter<F: Fn(&Progress)> {
    id: String,
    total: u64,
    last: AtomicU64,
    progress: F,
}

impl<F: Fn(&Progress)> Reporter<F> {
    fn new(id: &str, total: u64, progress: F) -> Self {
        let reporter = Reporter {
            id: id.to_string(),
            total,
            last: AtomicU64::new(0),
            progress,
        };
        reporter.tell(0);
        reporter
    }

    fn at(&self, done: u64) {
        if done >= self.total || done >= self.last.load(Ordering::Relaxed) + REPORT_EVERY {
            self.tell(done);
        }
    }

    fn tell(&self, done: u64) {
        self.last.store(done, Ordering::Relaxed);
        let now = Progress {
            id: self.id.clone(),
            done,
            total: self.total,
        };
        if let Ok(mut current) = CURRENT.lock() {
            *current = Some(now.clone());
        }
        (self.progress)(&now);
    }
}

impl<F: Fn(&Progress)> Drop for Reporter<F> {
    fn drop(&mut self) {
        if let Ok(mut current) = CURRENT.lock() {
            *current = None;
        }
    }
}

/// A part file beside its final name, removed unless it was moved into
/// place (Shoebox's paths.rs `ModelFile`).
struct Part {
    path: PathBuf,
    kept: bool,
}

impl Drop for Part {
    fn drop(&mut self) {
        if !self.kept {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// One file from one address, into its part file, moved into place once its
/// checksum matches.
async fn fetch<F: Fn(&Progress)>(
    dir: &Path,
    file: &File,
    url: &str,
    before: u64,
    report: &Reporter<F>,
) -> Result<(), String> {
    let mut part = Part {
        path: dir.join(format!("{}.part", file.name)),
        kept: false,
    };
    let mut response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    // Declared after `part`, so it is closed before the part file is removed.
    let mut out = std::fs::File::create(&part.path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut got = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if STOP.load(Ordering::Relaxed) {
            return Err(STOPPED.to_string());
        }
        got += chunk.len() as u64;
        if got > file.bytes {
            return Err("larger than expected".to_string());
        }
        hasher.update(&chunk);
        out.write_all(&chunk).map_err(|e| e.to_string())?;
        report.at(before + got);
    }
    out.sync_all().map_err(|e| e.to_string())?;
    drop(out);
    if got != file.bytes || hex::encode(hasher.finalize()) != file.sha256 {
        return Err("checksum does not match".to_string());
    }
    std::fs::rename(&part.path, dir.join(file.name)).map_err(|e| e.to_string())?;
    part.kept = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    /// Answers `GET /<name>` with that body, and anything else with 404.
    /// Counts the requests it answered.
    fn serve(bodies: Vec<(&'static str, Vec<u8>)>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let counted = count.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                counted.fetch_add(1, Ordering::SeqCst);
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .trim_start_matches('/');
                let mut stream = stream;
                match bodies.iter().find(|(name, _)| *name == path) {
                    Some((_, body)) => {
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    None => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                }
            }
        });
        (base, count)
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    fn file(name: &'static str, url: String, body: &[u8]) -> File {
        File {
            name,
            url: leak(url),
            fallback: None,
            sha256: leak(hex::encode(Sha256::digest(body))),
            bytes: body.len() as u64,
        }
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn a_file_that_matches_lands_under_its_name() {
        let body = vec![7u8; 3 << 20];
        let (base, _) = serve(vec![("a.onnx", body.clone())]);
        let tmp = tempfile::tempdir().unwrap();
        let heard = Mutex::new(Vec::new());
        let files = [file("a.onnx", format!("{base}/a.onnx"), &body)];

        let fetched = fetch_missing(tmp.path(), "lands", &files, |p| {
            heard.lock().unwrap().push(p.done)
        })
        .await
        .unwrap();

        assert_eq!(fetched, body.len() as u64);
        assert_eq!(std::fs::read(tmp.path().join("a.onnx")).unwrap(), body);
        assert_eq!(names_in(tmp.path()), ["a.onnx"]);
        // From 0 to the end, a few times between: not once a network chunk.
        let heard = heard.into_inner().unwrap();
        assert_eq!(heard.first(), Some(&0));
        assert_eq!(heard.last(), Some(&(body.len() as u64)));
        assert!(heard.len() <= 8, "{heard:?}");
        // Other tests download too, so only this one's is checked.
        assert!(current().is_none_or(|p| p.id != "lands"));
    }

    #[tokio::test]
    async fn a_wrong_file_is_not_kept() {
        let (base, _) = serve(vec![("a.onnx", b"not the model".to_vec())]);
        let tmp = tempfile::tempdir().unwrap();
        let files = [file(
            "a.onnx",
            format!("{base}/a.onnx"),
            b"the model, honest",
        )];

        let err = fetch_missing(tmp.path(), "test", &files, |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(names_in(tmp.path()).is_empty());
    }

    /// What is already there with the right checksum is not fetched again;
    /// a file of the right size with other bytes is.
    #[tokio::test]
    async fn only_what_is_missing_or_wrong_is_fetched() {
        let (a, b) = (b"model a".to_vec(), b"model b".to_vec());
        let (base, requests) = serve(vec![("a", a.clone()), ("b", b.clone())]);
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a"), &a).unwrap();
        std::fs::write(tmp.path().join("b"), b"model x").unwrap();
        let files = [
            file("a", format!("{base}/a"), &a),
            file("b", format!("{base}/b"), &b),
        ];

        let fetched = fetch_missing(tmp.path(), "test", &files, |_| {})
            .await
            .unwrap();
        assert_eq!(fetched, b.len() as u64);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert_eq!(std::fs::read(tmp.path().join("b")).unwrap(), b);

        assert_eq!(
            fetch_missing(tmp.path(), "test", &files, |_| {}).await,
            Ok(0)
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_fallback_is_tried_when_the_first_address_fails() {
        let body = b"mirrored".to_vec();
        let (base, _) = serve(vec![("original", body.clone())]);
        let tmp = tempfile::tempdir().unwrap();
        let mut files = [file("m.onnx", format!("{base}/mirror"), &body)];
        files[0].fallback = Some(leak(format!("{base}/original")));

        fetch_missing(tmp.path(), "test", &files, |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(tmp.path().join("m.onnx")).unwrap(), body);
    }

    #[tokio::test]
    async fn a_stopped_download_leaves_nothing_behind() {
        let body = vec![1u8; 4 << 20];
        let (base, _) = serve(vec![("big", body.clone())]);
        let tmp = tempfile::tempdir().unwrap();
        let files = [file("big.onnx", format!("{base}/big"), &body)];

        let err = fetch_missing(tmp.path(), "stopped", &files, |_| stop())
            .await
            .unwrap_err();
        assert_eq!(err, STOPPED);
        assert!(names_in(tmp.path()).is_empty());
        assert!(current().is_none_or(|p| p.id != "stopped"));
    }

    #[tokio::test]
    async fn the_runtime_is_not_downloaded_from_here() {
        let tmp = tempfile::tempdir().unwrap();
        let err = download(tmp.path(), model_catalog::GPU_RUNTIME, |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("arrives with"), "{err}");
        assert!(download(tmp.path(), "nothing", |_| {}).await.is_err());
        assert!(names_in(tmp.path()).is_empty());
    }

    /// A folder that is a link is left alone: the files it points at are kept
    /// somewhere else.
    #[cfg(windows)]
    #[tokio::test]
    async fn nothing_is_written_through_a_junction() {
        let tmp = tempfile::tempdir().unwrap();
        let kept = tmp.path().join("kept");
        std::fs::create_dir(&kept).unwrap();
        let link = tmp.path().join("linked");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&kept)
            .output()
            .is_ok_and(|o| o.status.success());
        if !made {
            eprintln!("could not make a junction here; skipped");
            return;
        }
        let files = [file("a", "https://127.0.0.1:9/a".into(), b"a")];
        let err = fetch_missing(&link, "test", &files, |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("link"), "{err}");
        assert!(names_in(&kept).is_empty());
    }
}
