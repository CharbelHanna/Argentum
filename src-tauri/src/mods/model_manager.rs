//! What is in the AI models folder, deleting it, and clearing what nothing
//! uses. Ours.
//!
//! The list of what can be there is `model_catalog`. This reads the folder
//! against it for Settings > General > AI Models, removes one entry's files on
//! request, and clears the leftovers: anything in the folder no entry claims.
//! Downloading is `model_download`'s.
//!
//! WHAT IT WILL DELETE
//!
//! An entry's files: only the names a catalogue entry lists, joined onto the
//! models folder (or the entry's own folder in it), and only after checking
//! each is a bare name, is not a link, and resolves to something directly
//! inside that folder. The settings page sends an id, never a path, so nothing
//! it says can name a file outside the list. A file two entries list (CLIP's
//! word list, Photo tagging's and the Text mask's) stays while the other entry
//! has all its files.
//!
//! Leftovers: only what `leftovers` lists — what no entry claims, such as a
//! half-finished download, a temporary file a feature left, a model a removed
//! feature fetched, or an old copy their code would rename and already has the
//! new name of. The page shows that list first and sends back the names it
//! showed; a name that is not a leftover when it arrives is skipped. Links are
//! never listed and never followed: one points at files kept somewhere else.
//!
//! WHEN IT CANNOT
//!
//! A model that is loaded can usually be deleted anyway: ONNX Runtime reads the
//! file into memory, and the feature keeps working until the app closes. Two
//! things cannot go while Argentum runs, and are removed the next time it
//! starts instead, before anything can load them:
//!
//! - anything Windows refuses to delete because it is open, and
//! - the graphics card runtime once this run has pointed ONNX Runtime at it
//!   (see `gpu_runtime::held_until_exit`). Windows might well let that one go,
//!   and then the next AI feature used would crash looking for it.
//!
//! The page says so, and shows the entry as waiting for a restart, rather than
//! reporting a deletion that has not happened.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::Manager;

use super::model_catalog::{self, CATALOGUE, GPU_RUNTIME, Model, RENAMED, Stored};
use super::model_download::{self, Progress};

/// Ids waiting to be removed at the next start, one per line.
const PENDING: &str = "argentum-remove-at-start.txt";

/// The folder every AI feature downloads into. `model_download::ensure_in_app`
/// asks here; `gpu_runtime` and `selection_models::dir` name this same one.
///
/// Not created here: looking at the settings page should not make a folder.
pub fn models_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("models"))
        .map_err(|e| e.to_string())
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OnDisk {
    Downloaded,
    /// Some of an entry's files and not others, or a file of the wrong size:
    /// an interrupted download.
    Partial,
    Missing,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: &'static str,
    pub label: &'static str,
    pub purpose: &'static str,
    pub on_disk: OnDisk,
    /// Size on disk now.
    pub bytes: u64,
    /// What a download fetches now: the files not yet there at their size.
    pub download_bytes: u64,
    /// Deleting it was refused while Argentum runs; it goes at the next start.
    pub removing_at_next_start: bool,
}

/// Something in the models folder no catalogue entry claims.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    /// Where it is inside the models folder, `/` between folder and name.
    pub name: String,
    pub bytes: u64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub models: Vec<ModelStatus>,
    /// Everything in the folder, listed or not.
    pub total_bytes: u64,
    /// The part of that `other` adds up to.
    pub other_bytes: u64,
    /// What no entry claims: what Clear deletes.
    pub other: Vec<Leftover>,
    /// The download running now, for a page opened after it began.
    pub downloading: Option<Progress>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Deleted,
    /// Could not go now; it is removed the next time Argentum starts.
    AtNextStart,
    NotThere,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Cleared {
    pub freed_bytes: u64,
    /// What could not be deleted, each with why.
    pub left: Vec<String>,
}

/// A single path component, with nothing that could climb out of the folder.
pub(super) fn is_bare_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':', '\0'])
        && Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name)
}

/// Size of a file, or of a folder and everything under it. Links count as
/// nothing and are not followed.
fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.file_type().is_symlink() {
        0
    } else if meta.is_dir() {
        fs::read_dir(path)
            .map(|entries| entries.flatten().map(|e| size_of(&e.path())).sum())
            .unwrap_or(0)
    } else {
        meta.len()
    }
}

fn entries(dir: &Path) -> impl Iterator<Item = fs::DirEntry> {
    fs::read_dir(dir).into_iter().flatten().flatten()
}

/// Real folders in `dir` whose names start with `prefix`.
fn folders_starting_with(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = entries(dir)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(prefix))
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

/// The folder an entry's files sit in, before any check.
fn files_dir(dir: &Path, model: &Model) -> PathBuf {
    match model.stored.folder() {
        Some(folder) => dir.join(folder),
        None => dir.to_path_buf(),
    }
}

/// What of this entry is on disk now, at any size.
fn present(dir: &Path, model: &Model) -> Vec<PathBuf> {
    match model.stored {
        Stored::FoldersStartingWith { prefix, .. } => folders_starting_with(dir, prefix),
        stored => {
            let inside = files_dir(dir, model);
            stored
                .files()
                .iter()
                .map(|f| inside.join(f.name))
                .filter(|path| fs::symlink_metadata(path).is_ok())
                .collect()
        }
    }
}

/// Every one of the entry's files is here, at its size.
fn complete(dir: &Path, model: &Model) -> bool {
    let inside = files_dir(dir, model);
    let files = model.stored.files();
    !files.is_empty()
        && files.iter().all(|f| {
            fs::metadata(inside.join(f.name)).is_ok_and(|m| m.is_file() && m.len() == f.bytes)
        })
}

/// A file of `model`'s that another entry lists too and has all of its own
/// files for, like CLIP's word list while Photo tagging is downloaded beside
/// the Text mask. It is that entry's as much as this one's: deleting `model`
/// leaves it, and it alone does not make `model` look downloaded.
fn kept_for_another(dir: &Path, model: &Model, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    CATALOGUE.iter().any(|other| {
        other.id != model.id
            && other.stored.folder() == model.stored.folder()
            && other.stored.files().iter().any(|f| f.name == name)
            && complete(dir, other)
    })
}

/// What of this entry is on disk and is its alone to delete.
fn owned(dir: &Path, model: &Model) -> Vec<PathBuf> {
    present(dir, model)
        .into_iter()
        .filter(|path| !kept_for_another(dir, model, path))
        .collect()
}

/// Does some entry claim this name directly in `dir`? An entry's own folder
/// is claimed here; what is inside it is sorted out by `find_leftovers`.
fn claimed(dir: &Path, name: &str, is_dir: bool) -> bool {
    name == PENDING
        || RENAMED
            .iter()
            .any(|(old, current)| *old == name && fs::symlink_metadata(dir.join(current)).is_err())
        || CATALOGUE.iter().any(|m| match m.stored {
            Stored::Files(files) => files.iter().any(|f| f.name == name),
            Stored::Folder { name: folder, .. } => is_dir && folder == name,
            Stored::FoldersStartingWith { prefix, .. } => is_dir && name.starts_with(prefix),
        })
}

fn pending(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join(PENDING))
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn set_pending(dir: &Path, ids: &[String]) -> io::Result<()> {
    let path = dir.join(PENDING);
    if ids.is_empty() {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        fs::write(path, ids.join("\n") + "\n")
    }
}

/// Every catalogue entry against what is in `dir`.
pub fn list(dir: &Path) -> Listing {
    let waiting: HashSet<String> = pending(dir).into_iter().collect();
    let mut models = Vec::new();
    for model in CATALOGUE {
        let found = present(dir, model);
        let (on_disk, download_bytes) = match model.stored {
            Stored::FoldersStartingWith { bytes, .. } if found.is_empty() => {
                (OnDisk::Missing, bytes)
            }
            Stored::FoldersStartingWith { .. } => (OnDisk::Downloaded, 0),
            stored => {
                let inside = files_dir(dir, model);
                let to_fetch: u64 = stored
                    .files()
                    .iter()
                    .filter(|f| {
                        !fs::metadata(inside.join(f.name))
                            .is_ok_and(|m| m.is_file() && m.len() == f.bytes)
                    })
                    .map(|f| f.bytes)
                    .sum();
                let on_disk = if owned(dir, model).is_empty() {
                    OnDisk::Missing
                } else if to_fetch > 0 {
                    OnDisk::Partial
                } else {
                    OnDisk::Downloaded
                };
                (on_disk, to_fetch)
            }
        };
        if on_disk == OnDisk::Missing && !model.listed_when_missing {
            continue;
        }
        models.push(ModelStatus {
            id: model.id,
            label: model.label,
            purpose: model.purpose,
            on_disk,
            bytes: found.iter().map(|p| size_of(p)).sum(),
            download_bytes,
            removing_at_next_start: on_disk != OnDisk::Missing && waiting.contains(model.id),
        });
    }

    let total_bytes = entries(dir).map(|e| size_of(&e.path())).sum();
    let other = leftovers(dir);
    Listing {
        models,
        total_bytes,
        other_bytes: other.iter().map(|l| l.bytes).sum(),
        other,
        downloading: None,
    }
}

/// `list`, with the download running now.
pub fn list_for_app(app: &tauri::AppHandle) -> Result<Listing, String> {
    let mut listing = list(&models_dir(app)?);
    listing.downloading = model_download::current();
    Ok(listing)
}

/// A leftover: where it is, the entry folder it is in (if any), and how the
/// page names it.
struct Found {
    path: PathBuf,
    folder: Option<&'static str>,
    leftover: Leftover,
}

/// Everything in `dir` no entry claims, and everything in an entry's own
/// folder that is not one of its files. Links are left out, and an entry's
/// folder that is a link is not looked into.
fn find_leftovers(dir: &Path) -> Vec<Found> {
    let mut found = Vec::new();
    for entry in entries(dir) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let own_folder = CATALOGUE
            .iter()
            .find(|m| kind.is_dir() && m.stored.folder() == Some(name.as_str()));
        if let Some(model) = own_folder {
            let files = model.stored.files();
            for inner in entries(&entry.path()) {
                if inner.file_type().is_ok_and(|t| t.is_symlink()) {
                    continue;
                }
                let inner_name = inner.file_name().to_string_lossy().into_owned();
                if !files.iter().any(|f| f.name == inner_name) {
                    found.push(Found {
                        path: inner.path(),
                        folder: model.stored.folder(),
                        leftover: Leftover {
                            name: format!("{name}/{inner_name}"),
                            bytes: size_of(&inner.path()),
                        },
                    });
                }
            }
        } else if !claimed(dir, &name, kind.is_dir()) {
            found.push(Found {
                path: entry.path(),
                folder: None,
                leftover: Leftover {
                    name,
                    bytes: size_of(&entry.path()),
                },
            });
        }
    }
    found.sort_by(|a, b| a.leftover.name.cmp(&b.leftover.name));
    found
}

/// What Clear would delete, by name and size.
pub fn leftovers(dir: &Path) -> Vec<Leftover> {
    find_leftovers(dir)
        .into_iter()
        .map(|f| f.leftover)
        .collect()
}

/// Windows' ways of saying a file is open somewhere.
fn in_use(e: &io::Error) -> bool {
    if cfg!(windows) {
        // 5: access denied, which is what deleting a loaded DLL returns.
        // 32 and 33: open, or locked, by someone who did not allow deletion.
        matches!(e.raw_os_error(), Some(5 | 32 | 33))
            || matches!(
                e.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy
            )
    } else {
        e.kind() == io::ErrorKind::ResourceBusy
    }
}

/// The folder an entry's files sit directly in, resolved: the models folder,
/// or the entry's own folder in it, which must be a real folder right there
/// and not a link to somewhere else.
fn entry_root(dir: &Path, folder: Option<&str>) -> Result<PathBuf, String> {
    let root = fs::canonicalize(dir).map_err(|e| e.to_string())?;
    let Some(folder) = folder else {
        return Ok(root);
    };
    let path = dir.join(folder);
    let meta = fs::symlink_metadata(&path).map_err(|e| format!("{folder}: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "{folder} is a link to another folder, so what is in it was left alone"
        ));
    }
    let real = fs::canonicalize(&path).map_err(|e| format!("{folder}: {e}"))?;
    if real.parent() != Some(root.as_path()) {
        return Err(format!(
            "Refusing to delete from {folder}: it is not inside the models folder"
        ));
    }
    Ok(real)
}

/// `target` is a bare name directly inside `root`, and not a link.
fn check_inside(root: &Path, target: &Path) -> Result<(), String> {
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| is_bare_name(n))
        .ok_or_else(|| format!("Refusing to delete {}: not a plain name", target.display()))?;
    let meta = fs::symlink_metadata(target).map_err(|e| format!("{name}: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "{name} is a link, not something Argentum downloaded, so it was left alone"
        ));
    }
    let real = fs::canonicalize(target).map_err(|e| format!("{name}: {e}"))?;
    if real.parent() != Some(root) {
        return Err(format!(
            "Refusing to delete {name}: it is not inside the models folder"
        ));
    }
    Ok(())
}

/// Delete a file, or a folder and what is in it (links inside are removed,
/// never followed).
fn remove(target: &Path) -> io::Result<()> {
    if fs::symlink_metadata(target).is_ok_and(|m| m.is_dir()) {
        fs::remove_dir_all(target)
    } else {
        fs::remove_file(target)
    }
}

enum Removal {
    Done,
    Refused,
}

/// Remove what of `model` is on disk. Refused if Windows would not let go of
/// some of it; anything else that goes wrong is an error.
fn remove_now(dir: &Path, model: &Model) -> Result<Removal, String> {
    let targets = owned(dir, model);
    if targets.is_empty() {
        return Ok(Removal::Done);
    }
    let root = entry_root(dir, model.stored.folder())?;
    let mut refused = false;
    for target in targets {
        check_inside(&root, &target)?;
        match remove(&target) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) if in_use(&e) => refused = true,
            Err(e) => {
                let name = target.file_name().unwrap_or_default().to_string_lossy();
                return Err(format!("Could not delete {name}: {e}"));
            }
        }
    }
    Ok(if refused {
        Removal::Refused
    } else {
        Removal::Done
    })
}

/// Delete one catalogue entry's files.
///
/// `held` says the entry must stay on disk until the app exits whatever
/// Windows would allow; it is then only marked for removal at the next start.
pub fn delete(dir: &Path, id: &str, held: bool) -> Result<Outcome, String> {
    let model = model_catalog::find(id).ok_or_else(|| format!("No AI model called {id}"))?;
    if owned(dir, model).is_empty() {
        return Ok(Outcome::NotThere);
    }

    let mut waiting = pending(dir);
    let outcome = if held {
        Removal::Refused
    } else {
        remove_now(dir, model)?
    };
    waiting.retain(|w| w != id);
    if let Removal::Refused = outcome {
        waiting.push(id.to_string());
    }
    set_pending(dir, &waiting).map_err(|e| e.to_string())?;

    Ok(match outcome {
        Removal::Done => Outcome::Deleted,
        Removal::Refused => Outcome::AtNextStart,
    })
}

/// `delete`, for the settings page: not while it is downloading, and asks
/// `gpu_runtime` whether the runtime is held this run.
pub fn delete_for_app(app: &tauri::AppHandle, id: &str) -> Result<Outcome, String> {
    if model_download::current().is_some_and(|p| p.id == id) {
        return Err("It is downloading. Stop the download first.".to_string());
    }
    let held = id == GPU_RUNTIME && super::gpu_runtime::held_until_exit();
    delete(&models_dir(app)?, id, held)
}

/// Delete the leftovers named, as `leftovers` lists them. A name that is not
/// a leftover now — a catalogue file, or something already gone — is skipped.
pub fn clear(dir: &Path, names: &[String]) -> Result<Cleared, String> {
    let mut cleared = Cleared {
        freed_bytes: 0,
        left: Vec::new(),
    };
    for found in find_leftovers(dir) {
        let name = &found.leftover.name;
        if !names.contains(name) {
            continue;
        }
        let removed = entry_root(dir, found.folder)
            .and_then(|root| check_inside(&root, &found.path))
            .and_then(|()| match remove(&found.path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) if in_use(&e) => Err("in use".to_string()),
                Err(e) => Err(e.to_string()),
            });
        match removed {
            Ok(()) => cleared.freed_bytes += found.leftover.bytes,
            Err(e) => cleared.left.push(format!("{name}: {e}")),
        }
    }
    Ok(cleared)
}

/// `clear`, for the settings page: never while a download is writing its
/// part file.
pub fn clear_for_app(app: &tauri::AppHandle, names: &[String]) -> Result<Cleared, String> {
    let _no_download = model_download::exclusive()
        .ok_or("A download is running. Clear the other files once it has finished.")?;
    clear(&models_dir(app)?, names)
}

/// Finish deletions that had to wait for a restart, and remove what a removed
/// feature left behind (`model_catalog::RETIRED`).
///
/// Called from `startup::init` before the graphics card runtime is pinned and
/// before any feature can load a model, which is the one moment nothing in
/// the folder is in use. Anything still refused stays on the list for next time.
pub fn finish_pending_removals(dir: &Path) {
    remove_retired(dir);
    let waiting = pending(dir);
    if waiting.is_empty() {
        return;
    }
    let mut still = Vec::new();
    for id in waiting {
        let Some(model) = model_catalog::find(&id) else {
            continue;
        };
        match remove_now(dir, model) {
            Ok(Removal::Done) => log::info!("[models] removed {id}, as asked last run"),
            Ok(Removal::Refused) => still.push(id),
            Err(e) => log::warn!("[models] could not remove {id}: {e}"),
        }
    }
    if let Err(e) = set_pending(dir, &still) {
        log::warn!("[models] could not update the removal list: {e}");
    }
}

/// Delete the files of features that are gone. Only plain files directly in
/// the folder, never through a link; a failure is logged and tried again at
/// the next start.
fn remove_retired(dir: &Path) {
    let Ok(root) = fs::canonicalize(dir) else {
        return;
    };
    for name in model_catalog::RETIRED {
        let path = dir.join(name);
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        let removed = check_inside(&root, &path)
            .and_then(|()| fs::remove_file(&path).map_err(|e| e.to_string()));
        match removed {
            Ok(()) => log::info!("[models] removed {name}: nothing uses it any more"),
            Err(e) => log::warn!("[models] could not remove {name}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::selection_models;

    fn write(dir: &Path, name: &str, bytes: usize) {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    /// A file of exactly this size, without writing its bytes.
    fn sized(dir: &Path, name: &str, bytes: u64) {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(path).unwrap().set_len(bytes).unwrap();
    }

    fn status<'a>(listing: &'a Listing, id: &str) -> Option<&'a ModelStatus> {
        listing.models.iter().find(|m| m.id == id)
    }

    fn names(listing: &[Leftover]) -> Vec<&str> {
        listing.iter().map(|l| l.name.as_str()).collect()
    }

    /// Every name the catalogue claims, as a path inside the models folder.
    fn every_catalogue_file() -> Vec<String> {
        let mut all = Vec::new();
        for model in CATALOGUE {
            match model.stored {
                Stored::FoldersStartingWith { prefix, .. } => {
                    all.push(format!("{prefix}1.22.0/onnxruntime.dll"))
                }
                stored => {
                    for file in stored.files() {
                        all.push(match stored.folder() {
                            Some(folder) => format!("{folder}/{}", file.name),
                            None => file.name.to_string(),
                        });
                    }
                }
            }
        }
        all
    }

    #[test]
    fn bare_names_only() {
        assert!(is_bare_name("u2net.onnx"));
        assert!(is_bare_name("onnxruntime-directml-"));
        for bad in [
            "",
            ".",
            "..",
            "../u2net.onnx",
            "a/b",
            "a\\b",
            "C:u2net.onnx",
            "nul\0",
        ] {
            assert!(!is_bare_name(bad), "{bad:?} passed");
        }
    }

    #[test]
    fn an_empty_or_missing_folder_lists_everything_as_not_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        for dir in [tmp.path().to_path_buf(), tmp.path().join("models")] {
            let listing = list(&dir);
            assert_eq!(listing.total_bytes, 0);
            assert!(listing.models.iter().all(|m| m.on_disk == OnDisk::Missing));
            let sky = status(&listing, "sky").unwrap();
            assert_eq!(sky.download_bytes, 175_997_079);
            let selection = status(&listing, model_catalog::SELECTION).unwrap();
            assert_eq!(selection.download_bytes, 184_115_050 + 86_559_030);
            // Only listed where it exists: it never does off Windows.
            assert!(status(&listing, GPU_RUNTIME).is_none());
            assert!(listing.other.is_empty());
        }
    }

    #[test]
    fn sizes_and_states_come_from_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        sized(dir, "omnilens_swinir_aodlib_256.onnx", 53_592_179);
        write(dir, "u2net.onnx", 1000);
        write(dir, "sam_vit_b_01ec64_encoder.onnx", 300);
        write(dir, "onnxruntime-directml-1.22.0/onnxruntime.dll", 50);
        write(dir, "onnxruntime-directml-1.21.0/onnxruntime.dll", 20);
        write(dir, ".u2net.onnx.download", 7);
        write(dir, "skyseg-u2net.onnx", 5);

        let listing = list(dir);
        let restore = status(&listing, "restore").unwrap();
        assert_eq!(
            (restore.on_disk, restore.bytes, restore.download_bytes),
            (OnDisk::Downloaded, 53_592_179, 0)
        );
        // There, but not at its size: an interrupted or damaged download.
        let foreground = status(&listing, "foreground").unwrap();
        assert_eq!(
            (
                foreground.on_disk,
                foreground.bytes,
                foreground.download_bytes
            ),
            (OnDisk::Partial, 1000, 175_997_641)
        );
        let subject = status(&listing, "subject").unwrap();
        assert_eq!(
            (subject.on_disk, subject.bytes, subject.download_bytes),
            (OnDisk::Partial, 300, 100_293_382 + 8_751_331)
        );
        let runtime = status(&listing, GPU_RUNTIME).unwrap();
        assert_eq!((runtime.on_disk, runtime.bytes), (OnDisk::Downloaded, 70));
        assert_eq!(status(&listing, "sky").unwrap().on_disk, OnDisk::Missing);
        // Their sky model under its old name is kept until their code renames it.
        assert_eq!(names(&listing.other), [".u2net.onnx.download"]);
        assert_eq!(listing.other_bytes, 7);
        assert_eq!(listing.total_bytes, 53_592_179 + 1000 + 300 + 70 + 7 + 5);
    }

    /// The selection models sit in their own folder; what else is in it is a
    /// leftover, and none of it is "other" the way the whole folder was before
    /// it had an entry.
    #[test]
    fn the_selection_models_are_listed_in_their_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let folder = selection_models::FOLDER;
        let files = selection_models::files();
        let (small, rest): (Vec<&model_catalog::File>, Vec<_>) =
            files.iter().partition(|f| f.bytes < 1 << 20);
        for file in &small {
            sized(dir, &format!("{folder}/{}", file.name), file.bytes);
        }
        write(dir, &format!("{folder}/vision_encoder.onnx_data.part"), 9);

        let listing = list(dir);
        let selection = status(&listing, model_catalog::SELECTION).unwrap();
        assert_eq!(selection.on_disk, OnDisk::Partial);
        assert_eq!(selection.bytes, small.iter().map(|f| f.bytes).sum::<u64>());
        assert_eq!(
            selection.download_bytes,
            rest.iter().map(|f| f.bytes).sum::<u64>()
        );
        assert_eq!(
            names(&listing.other),
            [format!("{folder}/vision_encoder.onnx_data.part").as_str()]
        );
        assert_eq!(listing.other_bytes, 9);
    }

    #[test]
    fn deleting_removes_that_entry_and_nothing_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "clip_model.onnx", 10);
        write(dir, "clip_tokenizer.json", 10);
        write(dir, "u2net.onnx", 10);
        write(dir, "unrelated.txt", 10);

        assert_eq!(delete(dir, "tagging", false), Ok(Outcome::Deleted));
        assert!(!dir.join("clip_model.onnx").exists());
        assert!(!dir.join("clip_tokenizer.json").exists());
        assert!(dir.join("u2net.onnx").exists());
        assert!(dir.join("unrelated.txt").exists());

        assert_eq!(delete(dir, "tagging", false), Ok(Outcome::NotThere));
        assert!(delete(dir, "../unrelated.txt", false).is_err());
        assert!(delete(dir, "unrelated.txt", false).is_err());
    }

    /// CLIP's word list is Photo tagging's and the Text mask's: deleting one
    /// leaves it while the other has all its files, and it alone does not make
    /// the deleted one look downloaded.
    #[test]
    fn a_file_two_entries_list_stays_for_the_one_still_whole() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for id in ["tagging", model_catalog::TEXT_MASKS] {
            for file in model_catalog::find(id).unwrap().stored.files() {
                sized(dir, file.name, file.bytes);
            }
        }
        let text = |listing: &Listing| status(listing, model_catalog::TEXT_MASKS).unwrap().on_disk;
        assert_eq!(text(&list(dir)), OnDisk::Downloaded);

        assert_eq!(
            delete(dir, model_catalog::TEXT_MASKS, false),
            Ok(Outcome::Deleted)
        );
        assert!(dir.join("clip_tokenizer.json").exists());
        assert!(!dir.join("efficientsam3_ev_m_decoder.onnx").exists());
        let listing = list(dir);
        assert_eq!(text(&listing), OnDisk::Missing);
        assert_eq!(
            status(&listing, "tagging").unwrap().on_disk,
            OnDisk::Downloaded
        );
        assert_eq!(
            delete(dir, model_catalog::TEXT_MASKS, false),
            Ok(Outcome::NotThere)
        );

        // Nothing else has it whole now, so it goes with Photo tagging.
        assert_eq!(delete(dir, "tagging", false), Ok(Outcome::Deleted));
        assert!(!dir.join("clip_tokenizer.json").exists());
    }

    #[test]
    fn the_runtime_goes_with_every_version_and_only_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "onnxruntime-directml-1.22.0/onnxruntime.dll", 10);
        write(dir, "onnxruntime-directml-1.21.0/onnxruntime.dll", 10);
        write(dir, "onnxruntime-directml-notes.txt", 10);
        write(dir, "onnxruntime-other/onnxruntime.dll", 10);

        assert_eq!(delete(dir, GPU_RUNTIME, false), Ok(Outcome::Deleted));
        assert!(!dir.join("onnxruntime-directml-1.22.0").exists());
        assert!(!dir.join("onnxruntime-directml-1.21.0").exists());
        assert!(dir.join("onnxruntime-directml-notes.txt").exists());
        assert!(dir.join("onnxruntime-other/onnxruntime.dll").exists());
    }

    /// Clear deletes what nothing uses, and never a file a listed model
    /// uses, even when it is asked to by name.
    #[test]
    fn clearing_deletes_leftovers_and_nothing_a_model_uses() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let kept = every_catalogue_file();
        for name in &kept {
            write(dir, name, 3);
        }
        write(dir, PENDING, 3);
        let leftovers_written = [
            (".u2net.onnx.download", 11),            // RapidRAW's temporary file
            ("realesrgan_x2plus.onnx.download", 12), // Super Resolution's
            ("u2net.onnx.part", 13),                 // ours, interrupted
            ("rawmakase-selection/vision_encoder.onnx.part", 14),
            ("skyseg-u2net.onnx", 15), // the old name, now the new one is here
            ("vitmatte_small_composition_1k.onnx", 16), // retired
            ("something-else/inside.bin", 17),
        ];
        for (name, bytes) in leftovers_written {
            write(dir, name, bytes);
        }

        let found = leftovers(dir);
        let mut expected: Vec<&str> = leftovers_written
            .iter()
            .map(|(name, _)| name.split_once("/inside").map_or(*name, |(d, _)| d))
            .collect();
        expected.sort();
        assert_eq!(names(&found), expected);

        // Asked for every leftover and every catalogue file by name.
        let mut asked: Vec<String> = found.iter().map(|l| l.name.clone()).collect();
        asked.extend(kept.iter().cloned());
        asked.push(PENDING.to_string());
        let cleared = clear(dir, &asked).unwrap();
        assert_eq!(cleared.freed_bytes, (11..=17).sum::<u64>());
        assert!(cleared.left.is_empty(), "{:?}", cleared.left);

        for name in &kept {
            assert!(dir.join(name).exists(), "{name} was deleted");
        }
        assert!(dir.join(PENDING).exists());
        assert!(leftovers(dir).is_empty());
        let listing = list(dir);
        assert_eq!(listing.other_bytes, 0);
    }

    /// Only what the page showed is cleared: something that turned up since
    /// stays until it is shown.
    #[test]
    fn clearing_deletes_only_the_names_asked_for() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "a.part", 1);
        write(dir, "b.part", 1);
        let cleared = clear(dir, &["a.part".to_string()]).unwrap();
        assert_eq!(cleared.freed_bytes, 1);
        assert!(!dir.join("a.part").exists());
        assert!(dir.join("b.part").exists());
    }

    /// Their sky model under its old name is that model until their code
    /// renames it: not a leftover, and not cleared.
    #[test]
    fn an_old_name_is_kept_until_the_new_one_is_there() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "skyseg-u2net.onnx", 5);
        assert!(leftovers(dir).is_empty());
        clear(dir, &["skyseg-u2net.onnx".to_string()]).unwrap();
        assert!(dir.join("skyseg-u2net.onnx").exists());
    }

    /// Copies kept outside the app and hard-linked into its folder: deleting
    /// the entry removes the app's names for them, and the kept copies stay
    /// whole. Clearing never lists them.
    #[test]
    fn hard_linked_copies_kept_elsewhere_survive() {
        let tmp = tempfile::tempdir().unwrap();
        let kept = tmp.path().join("kept");
        let dir = tmp.path().join("models");
        let folder = dir.join(selection_models::FOLDER);
        fs::create_dir_all(&kept).unwrap();
        fs::create_dir_all(&folder).unwrap();
        for file in selection_models::files() {
            fs::write(kept.join(file.name), file.name).unwrap();
            fs::hard_link(kept.join(file.name), folder.join(file.name)).unwrap();
        }

        assert!(leftovers(&dir).is_empty());
        assert_eq!(
            delete(&dir, model_catalog::SELECTION, false),
            Ok(Outcome::Deleted)
        );
        for file in selection_models::files() {
            assert!(!folder.join(file.name).exists());
            assert_eq!(fs::read_to_string(kept.join(file.name)).unwrap(), file.name);
        }
    }

    /// The selection folder as a junction to copies kept elsewhere: nothing in
    /// it is deleted or listed for clearing.
    #[cfg(windows)]
    #[test]
    fn a_junctioned_folder_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let kept = tmp.path().join("kept");
        let dir = tmp.path().join("models");
        fs::create_dir_all(&kept).unwrap();
        fs::create_dir_all(&dir).unwrap();
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(dir.join(selection_models::FOLDER))
            .arg(&kept)
            .output()
            .is_ok_and(|o| o.status.success());
        if !made {
            eprintln!("could not make a junction here; skipped");
            return;
        }
        for file in selection_models::files() {
            fs::write(kept.join(file.name), file.name).unwrap();
        }
        write(&kept, "stray.part", 1);

        assert!(leftovers(&dir).is_empty());
        assert!(delete(&dir, model_catalog::SELECTION, false).is_err());
        let asked = vec![format!("{}/stray.part", selection_models::FOLDER)];
        assert_eq!(clear(&dir, &asked).unwrap().freed_bytes, 0);
        for file in selection_models::files() {
            assert!(kept.join(file.name).exists(), "{}", file.name);
        }
        assert!(kept.join("stray.part").exists());
    }

    /// The runtime after this run has pointed ONNX Runtime at it: left in
    /// place, shown as waiting, and removed by the next start.
    #[test]
    fn a_held_entry_waits_for_the_next_start() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "onnxruntime-directml-1.22.0/onnxruntime.dll", 10);

        assert_eq!(delete(dir, GPU_RUNTIME, true), Ok(Outcome::AtNextStart));
        assert!(
            dir.join("onnxruntime-directml-1.22.0/onnxruntime.dll")
                .exists()
        );
        let listing = list(dir);
        assert!(
            status(&listing, GPU_RUNTIME)
                .unwrap()
                .removing_at_next_start
        );
        // The note to itself is not a model, and not "other files" either.
        assert_eq!(listing.other_bytes, 0);

        finish_pending_removals(dir);
        assert!(!dir.join("onnxruntime-directml-1.22.0").exists());
        assert!(!dir.join(PENDING).exists());
        assert!(status(&list(dir), GPU_RUNTIME).is_none());
    }

    /// What a removed feature downloaded goes at the next start, with no
    /// entry to ask for it; the models beside it stay.
    #[test]
    fn a_retired_model_goes_at_the_next_start() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let retired = model_catalog::RETIRED[0];
        write(dir, retired, 10);
        write(dir, "u2net.onnx", 10);
        // Until then nothing lists it: it is part of "other files".
        assert_eq!(list(dir).other_bytes, 10);

        finish_pending_removals(dir);
        assert!(!dir.join(retired).exists());
        assert!(dir.join("u2net.onnx").exists());
        assert_eq!(list(dir).other_bytes, 0);
    }

    /// Windows refuses to delete a file someone holds open without sharing
    /// deletion. That is reported as waiting for a restart, not as an error and
    /// not as done.
    #[cfg(windows)]
    #[test]
    fn a_file_windows_will_not_release_waits_for_the_next_start() {
        use std::os::windows::fs::OpenOptionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "lama_fp16.onnx", 10);
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ, and not FILE_SHARE_DELETE
            .open(dir.join("lama_fp16.onnx"))
            .unwrap();

        assert_eq!(delete(dir, "inpainting", false), Ok(Outcome::AtNextStart));
        assert!(dir.join("lama_fp16.onnx").exists());
        assert!(
            status(&list(dir), "inpainting")
                .unwrap()
                .removing_at_next_start
        );

        drop(held);
        finish_pending_removals(dir);
        assert!(!dir.join("lama_fp16.onnx").exists());
        assert!(!dir.join(PENDING).exists());
    }

    /// Deleting again once it is possible clears it from the waiting list.
    #[test]
    fn a_later_deletion_clears_the_waiting_list() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "u2net.onnx", 10);
        assert_eq!(delete(dir, "foreground", true), Ok(Outcome::AtNextStart));
        assert_eq!(delete(dir, "foreground", false), Ok(Outcome::Deleted));
        assert!(!dir.join(PENDING).exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_never_followed_out_of_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(outside.path(), "precious.onnx", 10);
        std::os::unix::fs::symlink(outside.path().join("precious.onnx"), dir.join("u2net.onnx"))
            .unwrap();

        assert!(delete(dir, "foreground", false).is_err());
        assert!(outside.path().join("precious.onnx").exists());
    }
}
