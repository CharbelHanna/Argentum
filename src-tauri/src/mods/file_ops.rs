//! What a library file operation touches, decided before anything on disk
//! changes. Ours.
//!
//! A photo travels with its files: its edits (`IMG.CR2.agdata`, and one
//! `IMG.CR2.<id>.agdata` per virtual copy), its EXIF cache (`IMG.CR2.agexif`)
//! and its XMP (`IMG.xmp`). Delete, copy, move and rename in
//! `file_management.rs` ask this module which files those are and where they
//! go, and do only the disk work themselves:
//!
//! - [`is_sidecar_of`]: which `.agdata` files hold a photo's edits.
//! - [`files_to_delete_with_associated`]: what "Delete with associated" trashes.
//! - [`xmp_still_used`] and [`xmp_after_rename`]: the XMP a RAW and its JPEG
//!   share stays with the one that stays (ours).
//! - [`Destinations`]: no file of one copy or move lands on a file already
//!   there or on another file of the same operation.
//! - [`same_folder_copy`]: the names of a photo duplicated in its own folder.
//! - [`paths_after_rename`]: the selection after a rename, one path per input.
//! - [`xmp_text`] and [`xmp_text_read`]: tag and label text as XMP stores it.
//! - [`trash_or_delete`]: how a library delete removes a file.
//!
//! The rules follow RapidRAW 1.6.7's fixes to `src-tauri/src/file_management.rs`
//! (fefb6a1b, 4f64511a); each copied rule names its commit.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::file_management::{parse_sidecar_name, parse_virtual_path};
use crate::formats::is_supported_image_file;

/// Why a file operation stopped before writing anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOpError {
    /// A file is already at the destination. Nothing is overwritten.
    DestinationExists(PathBuf),
    /// Two different files of one copy or move would land on the same path,
    /// such as two `IMG_0001.CR2` from different folders.
    SameDestination {
        first: PathBuf,
        second: PathBuf,
        destination: PathBuf,
    },
    /// The path has no file name to build a new one from.
    NoFileName(PathBuf),
}

impl fmt::Display for FileOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DestinationExists(path) => {
                write!(f, "File already exists at destination: {}", path.display())
            }
            Self::SameDestination {
                first,
                second,
                destination,
            } => write!(
                f,
                "{} and {} would both be written to {}",
                first.display(),
                second.display(),
                destination.display()
            ),
            Self::NoFileName(path) => {
                write!(f, "Could not read the file name of {}", path.display())
            }
        }
    }
}

impl std::error::Error for FileOpError {}

/// Whether the file `file_name` holds edits of the photo `image_name` beside
/// it: `IMG.CR2.agdata`, or a virtual copy's `IMG.CR2.a1b2c3.agdata`. The
/// edits of `IMG.CR2.jpg` (`IMG.CR2.jpg.agdata`) are not `IMG.CR2`'s.
// RapidRAW 4f64511a: src-tauri/src/file_management.rs (sidecars matched with parse_sidecar_name)
pub fn is_sidecar_of(file_name: &str, image_name: &str) -> bool {
    parse_sidecar_name(file_name).is_some_and(|(name, _)| name == image_name)
}

/// The name without extension that ties `file_name` to a photo for "Delete
/// with associated": `IMG` for `IMG.CR2`, `IMG.jpg`, `IMG.xmp`,
/// `IMG.CR2.agexif`, `IMG.CR2.agdata` and `IMG.CR2.a1b2c3.agdata`. None for a
/// file that is neither a photo nor one of its sidecars, which is never
/// deleted.
// RapidRAW 52138689 (1.6.6): src-tauri/src/file_management.rs `deletion_stem_for`;
// its virtual copy rule is now parse_sidecar_name's.
fn delete_stem(file_name: &str) -> Option<&str> {
    let image_name = parse_sidecar_name(file_name)
        .map(|(name, _)| name)
        .or_else(|| file_name.strip_suffix(".agexif"))
        .or_else(|| file_name.strip_suffix(".xmp"))
        .or_else(|| file_name.strip_suffix(".XMP"))
        .or_else(|| is_supported_image_file(file_name).then_some(file_name))?;
    Path::new(image_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
}

/// Every file "Delete with associated" moves to the trash for the photos
/// `paths` (a virtual copy counts as its photo): in each photo's own folder,
/// the photos and sidecars sharing its name without extension. Names are
/// matched per folder, so deleting `/a/IMG_1.CR2` with `/b/IMG_2.CR2` leaves
/// `/a/IMG_2.CR2` and `/b/IMG_1.CR2` alone. Sorted, each file once.
// RapidRAW fefb6a1b: src-tauri/src/file_management.rs (stems matched per folder)
pub fn files_to_delete_with_associated(paths: &[String]) -> Vec<PathBuf> {
    let mut names_by_folder: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for path in paths {
        let (photo, _) = parse_virtual_path(path);
        if let (Some(folder), Some(name)) = (
            photo.parent(),
            photo.file_stem().and_then(|stem| stem.to_str()),
        ) {
            names_by_folder
                .entry(folder.to_path_buf())
                .or_default()
                .insert(name.to_string());
        }
    }

    let mut files = Vec::new();
    for (folder, names) in &names_by_folder {
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
            if is_file
                && entry
                    .file_name()
                    .to_str()
                    .and_then(delete_stem)
                    .is_some_and(|stem| names.contains(stem))
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Whether `file` is an XMP, `IMG.xmp`: named after the photo's stem, so
/// `IMG.CR2` and `IMG.jpg` share it.
pub fn is_xmp(file: &Path) -> bool {
    file.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xmp"))
}

/// Whether a photo other than those in `leaving` still uses `xmp`: a photo
/// in its folder with its stem. Deleting, moving or renaming the RAW of a
/// RAW + JPEG pair leaves the JPEG its XMP. Names compare without case, and a
/// folder that cannot be read counts as using it: keeping the XMP is the safe
/// side.
pub fn xmp_still_used(xmp: &Path, leaving: &HashSet<PathBuf>) -> bool {
    let (Some(folder), Some(stem)) = (xmp.parent(), xmp.file_stem().and_then(|s| s.to_str()))
    else {
        return false;
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return true;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        entry
            .file_name()
            .to_str()
            .is_some_and(is_supported_image_file)
            && path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case(stem))
            && !leaving.contains(&path)
    })
}

/// How an XMP reaches one of its photos' new names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carry {
    Copy,
    Move,
}

/// What renaming photos does to the XMP they share: one file at each of
/// `targets`, the new names in order. Each but the last is a copy; the last
/// takes the file itself, unless a photo that kept the old name still uses it.
pub fn xmp_after_rename(targets: &[PathBuf], still_used: bool) -> Vec<(PathBuf, Carry)> {
    let mut unique: Vec<&PathBuf> = Vec::new();
    for target in targets {
        if !unique.contains(&target) {
            unique.push(target);
        }
    }
    let last = unique.len().saturating_sub(1);
    unique
        .into_iter()
        .enumerate()
        .map(|(i, target)| {
            let carry = if i == last && !still_used {
                Carry::Move
            } else {
                Carry::Copy
            };
            (target.clone(), carry)
        })
        .collect()
}

/// The files one copy or move will write, by destination. Each destination
/// takes one source: a path already on disk, or planned for another source of
/// the same operation, is refused, never overwritten.
#[derive(Debug, Default)]
pub struct Destinations {
    planned: HashMap<String, PathBuf>,
}

impl Destinations {
    /// Destinations compare without case: on Windows and macOS `IMG.CR2` and
    /// `img.CR2` are one file. Where they are two, refusing is the safe side.
    fn key(destination: &Path) -> String {
        destination.to_string_lossy().to_lowercase()
    }

    /// Whether anything is at `destination` on disk (a broken link counts), or
    /// a file of this operation is planned there.
    pub fn is_taken(&self, destination: &Path) -> bool {
        self.planned.contains_key(&Self::key(destination))
            || fs::symlink_metadata(destination).is_ok()
    }

    /// Plans `source` to be written at `destination`. `Ok(true)` when newly
    /// planned; `Ok(false)` when this same source is planned there already, as
    /// the `.xmp` a RAW and its JPEG share is when both are selected.
    // RapidRAW fefb6a1b: src-tauri/src/file_management.rs (destinations planned earlier are refused too)
    pub fn claim(&mut self, source: &Path, destination: &Path) -> Result<bool, FileOpError> {
        let key = Self::key(destination);
        match self.planned.get(&key) {
            Some(planned) if planned == source => Ok(false),
            Some(planned) => Err(FileOpError::SameDestination {
                first: planned.clone(),
                second: source.to_path_buf(),
                destination: destination.to_path_buf(),
            }),
            None if fs::symlink_metadata(destination).is_ok() => {
                Err(FileOpError::DestinationExists(destination.to_path_buf()))
            }
            None => {
                self.planned.insert(key, source.to_path_buf());
                Ok(true)
            }
        }
    }
}

/// The name of `file_name`, one of `photo`'s files, for the photo's copy
/// `copy`: `IMG.CR2.agdata` becomes `IMG_copy_1.CR2.agdata`, and the XMP,
/// named after the stem alone (`IMG.xmp`), becomes `IMG_copy_1.xmp`. None for
/// a file named after neither, which is left out rather than written over
/// itself.
fn companion_name(file_name: &str, photo: &FileName, copy: &FileName) -> Option<String> {
    if let Some(rest) = file_name.strip_prefix(photo.name.as_str()) {
        return Some(format!("{}{rest}", copy.name));
    }
    file_name
        .strip_prefix(photo.stem.as_str())
        .filter(|rest| rest.starts_with('.'))
        .map(|rest| format!("{}{rest}", copy.stem))
}

/// A file name and its stem.
struct FileName {
    name: String,
    stem: String,
}

/// Where each of `files` goes when `photo` is duplicated in its own folder.
/// `files` are the photo and its own files, as `find_all_associated_files`
/// lists them. The copy is `<stem>_copy_<n>`, at the first `n` where neither
/// the photo nor any of its files is taken, so the copy never lands on another
/// photo's sidecar or XMP, and the XMP is never copied onto itself. Each
/// destination is claimed in `destinations`.
pub fn same_folder_copy(
    photo: &Path,
    files: &[PathBuf],
    destinations: &mut Destinations,
) -> Result<Vec<(PathBuf, PathBuf)>, FileOpError> {
    let no_name = || FileOpError::NoFileName(photo.to_path_buf());
    let folder = photo.parent().ok_or_else(no_name)?;
    let original = FileName {
        name: photo
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(no_name)?
            .to_string(),
        stem: photo
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(no_name)?
            .to_string(),
    };
    let extension = photo
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");

    let mut n: usize = 1;
    loop {
        let stem = format!("{}_copy_{n}", original.stem);
        let copy = FileName {
            name: format!("{stem}.{extension}"),
            stem,
        };
        let plan: Vec<(PathBuf, PathBuf)> = files
            .iter()
            .filter_map(|file| {
                let name = file.file_name()?.to_str()?;
                let destination = folder.join(companion_name(name, &original, &copy)?);
                Some((file.clone(), destination))
            })
            .collect();
        if plan
            .iter()
            .all(|(_, destination)| !destinations.is_taken(destination))
        {
            for (source, destination) in &plan {
                destinations.claim(source, destination)?;
            }
            return Ok(plan);
        }
        n += 1;
    }
}

/// The selection after a rename: one path per path in `inputs`, in order. A
/// renamed photo takes its new path, a virtual copy keeps its `?vc=` id, and a
/// photo that was not renamed keeps its path, so the interface can map the
/// result back to its selection by index.
// RapidRAW 4f64511a: src-tauri/src/file_management.rs (rename_files returns one path per input)
pub fn paths_after_rename(inputs: &[String], renames: &HashMap<String, String>) -> Vec<String> {
    inputs
        .iter()
        .map(|input| {
            let (photo, copy_id) = match input.rsplit_once("?vc=") {
                Some((photo, id)) => (photo, format!("?vc={id}")),
                None => (input.as_str(), String::new()),
            };
            let renamed = renames.get(photo).map_or(photo, String::as_str);
            format!("{renamed}{copy_id}")
        })
        .collect()
}

/// `text` as XMP stores it in an element or attribute value: `R&D` is written
/// `R&amp;D`, so the file stays valid XML.
// RapidRAW fefb6a1b: src-tauri/src/file_management.rs (escape on write)
pub fn xmp_text(text: &str) -> Cow<'_, str> {
    quick_xml::escape::escape(text)
}

/// Text read from an XMP element or attribute value: `R&amp;D` is `R&D`. Text
/// that is not valid escaped XML, such as a bare `&` written before tags were
/// escaped, is kept as written.
// RapidRAW fefb6a1b: src-tauri/src/file_management.rs (unescape on read)
pub fn xmp_text_read(raw: &str) -> String {
    quick_xml::escape::unescape(raw).map_or_else(|_| raw.to_string(), Cow::into_owned)
}

/// Moves `paths` to the system trash. Where the trash refuses (a drive or
/// share without one), deletes them permanently instead and logs it, as the
/// library always has; the delete dialog says the delete is permanent.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn trash_or_delete(paths: &[PathBuf]) {
    let Err(trash_error) = trash::delete_all(paths) else {
        return;
    };
    log::warn!("Failed to move files to trash: {trash_error}. Falling back to permanent delete.");
    for path in paths {
        let deleted = if path.is_dir() {
            fs::remove_dir_all(path)
        } else if path.is_file() {
            fs::remove_file(path)
        } else {
            continue;
        };
        if let Err(e) = deleted {
            log::warn!("Failed to delete {}: {}", path.display(), e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_management::{
        copy_files, files_to_delete_from_disk, find_all_associated_files, finish_import,
        move_files_on_disk, rename_files_on_disk, sync_metadata_from_xmp, sync_metadata_to_xmp,
    };
    use crate::image_processing::ImageMetadata;

    fn is_sidecar(path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "agdata")
    }

    /// A sidecar is written as a real one, carrying `text` as its tag, because a
    /// sidecar that does not read gets a rescue copy beside it (mods/salvage.rs).
    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = if is_sidecar(path) {
            serde_json::json!({ "version": 1, "rating": 0, "adjustments": {}, "tags": [text] })
                .to_string()
        } else {
            text.to_string()
        };
        fs::write(path, text).unwrap();
    }

    /// The text `write` was given.
    fn read(path: &Path) -> String {
        let text = fs::read_to_string(path).unwrap();
        if !is_sidecar(path) {
            return text;
        }
        let meta: ImageMetadata = serde_json::from_str(&text).unwrap();
        meta.tags.unwrap().remove(0)
    }

    fn names_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn sidecars_belong_to_the_photo_with_exactly_that_name() {
        assert!(is_sidecar_of("IMG.CR2.agdata", "IMG.CR2"));
        assert!(is_sidecar_of("IMG.CR2.a1b2c3.agdata", "IMG.CR2"));
        assert!(!is_sidecar_of("IMG.CR2.jpg.agdata", "IMG.CR2"));
        assert!(!is_sidecar_of("IMG.CR2.jpg.a1b2c3.agdata", "IMG.CR2"));
        assert!(is_sidecar_of("IMG.CR2.jpg.a1b2c3.agdata", "IMG.CR2.jpg"));
        assert!(!is_sidecar_of("IMG.CR2.agexif", "IMG.CR2"));
        assert!(!is_sidecar_of("IMG.CR2", "IMG.CR2"));
    }

    #[test]
    fn a_photo_named_like_another_with_more_on_the_end_keeps_its_edits() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        for name in [
            "IMG.CR2",
            "IMG.CR2.agdata",
            "IMG.CR2.a1b2c3.agdata",
            "IMG.CR2.agexif",
            "IMG.xmp",
            "IMG.CR2.jpg",
            "IMG.CR2.jpg.agdata",
            "IMG.CR2.jpg.b2c3d4.agdata",
        ] {
            write(&folder.join(name), name);
        }

        let mut found = find_all_associated_files(&folder.join("IMG.CR2")).unwrap();
        found.sort();
        let mut expected: Vec<PathBuf> = [
            "IMG.CR2",
            "IMG.CR2.agdata",
            "IMG.CR2.a1b2c3.agdata",
            "IMG.CR2.agexif",
            "IMG.xmp",
        ]
        .iter()
        .map(|name| folder.join(name))
        .collect();
        expected.sort();
        assert_eq!(found, expected);
    }

    #[test]
    fn delete_with_associated_matches_names_folder_by_folder() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        for path in [
            a.join("IMG_1.CR2"),
            a.join("IMG_1.jpg"),
            a.join("IMG_1.CR2.agdata"),
            a.join("IMG_1.CR2.a1b2c3.agdata"),
            a.join("IMG_2.CR2"),
            a.join("IMG_2.CR2.agdata"),
            a.join("notes.txt"),
            b.join("IMG_1.CR2"),
            b.join("IMG_1.xmp"),
            b.join("IMG_2.CR2"),
            b.join("IMG_2.xmp"),
            b.join("IMG_2.CR2.agexif"),
        ] {
            write(&path, "x");
        }

        let trashed = files_to_delete_with_associated(&[
            text(&a.join("IMG_1.CR2")),
            format!("{}?vc=a1b2c3", text(&b.join("IMG_2.CR2"))),
        ]);

        let mut expected = vec![
            a.join("IMG_1.CR2"),
            a.join("IMG_1.jpg"),
            a.join("IMG_1.CR2.agdata"),
            a.join("IMG_1.CR2.a1b2c3.agdata"),
            b.join("IMG_2.CR2"),
            b.join("IMG_2.xmp"),
            b.join("IMG_2.CR2.agexif"),
        ];
        expected.sort();
        assert_eq!(trashed, expected);
    }

    #[test]
    fn a_destination_is_taken_once_whatever_its_case() {
        let dir = tempfile::tempdir().unwrap();
        let mut destinations = Destinations::default();
        let first = dir.path().join("a").join("IMG.CR2");
        let second = dir.path().join("b").join("img.CR2");
        let destination = dir.path().join("c").join("IMG.CR2");

        assert_eq!(destinations.claim(&first, &destination), Ok(true));
        assert_eq!(destinations.claim(&first, &destination), Ok(false));
        assert_eq!(
            destinations.claim(&second, &dir.path().join("c").join("img.cr2")),
            Err(FileOpError::SameDestination {
                first: first.clone(),
                second: second.clone(),
                destination: dir.path().join("c").join("img.cr2"),
            })
        );

        write(&dir.path().join("c").join("other.jpg"), "there");
        assert_eq!(
            destinations.claim(&first, &dir.path().join("c").join("other.jpg")),
            Err(FileOpError::DestinationExists(
                dir.path().join("c").join("other.jpg")
            ))
        );
    }

    #[test]
    fn copying_two_same_named_photos_into_one_folder_overwrites_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a").join("IMG.CR2");
        let b = dir.path().join("b").join("IMG.CR2");
        let target = dir.path().join("target");
        write(&a, "photo a");
        write(&b, "photo b");
        write(&dir.path().join("b").join("IMG.CR2.agdata"), "edits b");
        fs::create_dir_all(&target).unwrap();

        let result = copy_files(vec![text(&a), text(&b)], text(&target));

        let error = result.unwrap_err();
        assert!(error.contains("would both be written to"), "{error}");
        assert!(names_in(&target).is_empty());
        assert_eq!(read(&a), "photo a");
        assert_eq!(read(&b), "photo b");
    }

    #[test]
    fn moving_two_same_named_photos_into_one_folder_overwrites_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a").join("IMG.CR2");
        let b = dir.path().join("b").join("IMG.CR2");
        let target = dir.path().join("target");
        write(&a, "photo a");
        write(&b, "photo b");
        fs::create_dir_all(&target).unwrap();

        let result = move_files_on_disk(&[text(&a), text(&b)], &text(&target));

        assert!(result.is_err());
        assert!(names_in(&target).is_empty());
        assert_eq!(read(&a), "photo a");
        assert_eq!(read(&b), "photo b");
    }

    #[test]
    fn a_file_already_at_the_destination_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("a").join("IMG.CR2");
        let target = dir.path().join("target");
        write(&photo, "new");
        write(&target.join("IMG.CR2"), "already there");

        let copied = copy_files(vec![text(&photo)], text(&target));
        let moved = move_files_on_disk(&[text(&photo)], &text(&target));

        assert!(
            copied
                .unwrap_err()
                .contains("File already exists at destination")
        );
        assert!(
            moved
                .unwrap_err()
                .contains("File already exists at destination")
        );
        assert_eq!(read(&target.join("IMG.CR2")), "already there");
        assert_eq!(read(&photo), "new");
    }

    #[test]
    fn a_raw_and_its_jpeg_sharing_one_xmp_move_together() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("a");
        let target = dir.path().join("target");
        write(&folder.join("IMG.CR2"), "raw");
        write(&folder.join("IMG.jpg"), "jpeg");
        write(&folder.join("IMG.xmp"), "xmp");
        write(&folder.join("IMG.CR2.agdata"), "edits");
        fs::create_dir_all(&target).unwrap();

        let renames = move_files_on_disk(
            &[text(&folder.join("IMG.CR2")), text(&folder.join("IMG.jpg"))],
            &text(&target),
        )
        .unwrap();

        assert_eq!(
            names_in(&target),
            ["IMG.CR2", "IMG.CR2.agdata", "IMG.jpg", "IMG.xmp"]
        );
        assert_eq!(read(&target.join("IMG.xmp")), "xmp");
        assert!(names_in(&folder).is_empty());
        assert_eq!(
            renames.get(&text(&folder.join("IMG.CR2"))),
            Some(&text(&target.join("IMG.CR2")))
        );
    }

    #[test]
    fn a_photo_duplicated_in_its_folder_gets_its_own_files() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(&folder.join("IMG.CR2"), "raw");
        write(&folder.join("IMG.CR2.agdata"), "edits");
        write(&folder.join("IMG.xmp"), "xmp");
        // A leftover sidecar of a deleted first copy: its name is taken.
        write(&folder.join("IMG_copy_1.CR2.agdata"), "leftover");

        copy_files(vec![text(&folder.join("IMG.CR2"))], text(folder)).unwrap();

        assert_eq!(read(&folder.join("IMG_copy_2.CR2")), "raw");
        assert_eq!(read(&folder.join("IMG_copy_2.CR2.agdata")), "edits");
        assert_eq!(read(&folder.join("IMG_copy_2.xmp")), "xmp");
        assert_eq!(read(&folder.join("IMG.xmp")), "xmp");
        assert_eq!(read(&folder.join("IMG_copy_1.CR2.agdata")), "leftover");
        assert!(!folder.join("IMG_copy_1.CR2").exists());
    }

    #[test]
    fn the_selection_after_a_rename_has_one_path_per_input() {
        let renames = HashMap::from([("/p/a.CR2".to_string(), "/p/trip_1.CR2".to_string())]);
        let inputs = [
            "/p/a.CR2".to_string(),
            "/p/a.CR2?vc=a1b2c3".to_string(),
            "/p/b.CR2".to_string(),
        ];
        assert_eq!(
            paths_after_rename(&inputs, &renames),
            ["/p/trip_1.CR2", "/p/trip_1.CR2?vc=a1b2c3", "/p/b.CR2"]
        );
    }

    #[test]
    fn a_rename_moves_only_the_photos_own_edits_and_counts_each_file_once() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        for name in [
            "IMG.CR2",
            "IMG.CR2.agdata",
            "IMG.CR2.a1b2c3.agdata",
            "IMG.CR2.agexif",
            "IMG.CR2.jpg",
            "IMG.CR2.jpg.agdata",
            "OTHER.CR2",
        ] {
            write(&folder.join(name), name);
        }
        let photo = text(&folder.join("IMG.CR2"));
        let inputs = [
            photo.clone(),
            format!("{photo}?vc=a1b2c3"),
            text(&folder.join("OTHER.CR2")),
        ];

        let (new_paths, _) = rename_files_on_disk(&inputs, "trip_{sequence}").unwrap();

        assert_eq!(
            new_paths,
            [
                text(&folder.join("trip_1.CR2")),
                format!("{}?vc=a1b2c3", text(&folder.join("trip_1.CR2"))),
                text(&folder.join("trip_2.CR2")),
            ]
        );
        assert_eq!(
            names_in(folder),
            [
                "IMG.CR2.jpg",
                "IMG.CR2.jpg.agdata",
                "trip_1.CR2",
                "trip_1.CR2.a1b2c3.agdata",
                "trip_1.CR2.agdata",
                "trip_1.CR2.agexif",
                "trip_2.CR2",
            ]
        );
        assert_eq!(read(&folder.join("trip_1.CR2.agdata")), "IMG.CR2.agdata");
        assert_eq!(
            read(&folder.join("IMG.CR2.jpg.agdata")),
            "IMG.CR2.jpg.agdata"
        );
    }

    /// A photo open in another program cannot be renamed on Windows. Its edits
    /// must keep its name too, or the photo shows unedited.
    #[cfg(windows)]
    #[test]
    fn a_photo_that_cannot_be_renamed_keeps_its_edits() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(&folder.join("IMG.CR2"), "raw");
        write(&folder.join("IMG.CR2.agdata"), "edits");
        let photo = text(&folder.join("IMG.CR2"));

        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(folder.join("IMG.CR2"))
            .unwrap();
        let (new_paths, _) = rename_files_on_disk(std::slice::from_ref(&photo), "trip").unwrap();
        drop(held);

        assert_eq!(new_paths, [photo]);
        assert_eq!(names_in(folder), ["IMG.CR2", "IMG.CR2.agdata"]);
    }

    /// A RAW and its JPEG share `IMG.xmp`: deleting one leaves it to the other.
    #[test]
    fn deleting_the_raw_of_a_pair_leaves_the_jpeg_its_xmp() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        for name in [
            "IMG.CR2",
            "IMG.CR2.agdata",
            "IMG.CR2.agexif",
            "IMG.jpg",
            "IMG.xmp",
            "OTHER.CR2",
            "OTHER.xmp",
        ] {
            write(&folder.join(name), name);
        }
        let files =
            |names: &[&str]| -> Vec<PathBuf> { names.iter().map(|n| folder.join(n)).collect() };

        assert_eq!(
            files_to_delete_from_disk(&[text(&folder.join("IMG.CR2"))]),
            files(&["IMG.CR2", "IMG.CR2.agdata", "IMG.CR2.agexif"])
        );
        assert_eq!(
            files_to_delete_from_disk(&[
                text(&folder.join("IMG.CR2")),
                text(&folder.join("IMG.jpg"))
            ]),
            files(&[
                "IMG.CR2",
                "IMG.CR2.agdata",
                "IMG.CR2.agexif",
                "IMG.jpg",
                "IMG.xmp"
            ])
        );
        assert_eq!(
            files_to_delete_from_disk(&[text(&folder.join("OTHER.CR2"))]),
            files(&["OTHER.CR2", "OTHER.xmp"])
        );
    }

    #[test]
    fn moving_the_raw_of_a_pair_copies_the_xmp_it_shares() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("a");
        let target = dir.path().join("target");
        write(&folder.join("IMG.CR2"), "raw");
        write(&folder.join("IMG.jpg"), "jpeg");
        write(&folder.join("IMG.xmp"), "xmp");
        fs::create_dir_all(&target).unwrap();

        move_files_on_disk(&[text(&folder.join("IMG.CR2"))], &text(&target)).unwrap();

        assert_eq!(names_in(&target), ["IMG.CR2", "IMG.xmp"]);
        assert_eq!(names_in(&folder), ["IMG.jpg", "IMG.xmp"]);
        assert_eq!(read(&target.join("IMG.xmp")), "xmp");
    }

    #[test]
    fn a_renamed_photo_takes_its_xmp() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(&folder.join("IMG.CR2"), "raw");
        write(&folder.join("IMG.xmp"), "xmp");

        rename_files_on_disk(&[text(&folder.join("IMG.CR2"))], "trip").unwrap();

        assert_eq!(names_in(folder), ["trip.CR2", "trip.xmp"]);
        assert_eq!(read(&folder.join("trip.xmp")), "xmp");
    }

    #[test]
    fn renaming_the_raw_of_a_pair_copies_the_xmp_and_leaves_it_to_the_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(&folder.join("IMG.CR2"), "raw");
        write(&folder.join("IMG.jpg"), "jpeg");
        write(&folder.join("IMG.xmp"), "xmp");

        rename_files_on_disk(&[text(&folder.join("IMG.CR2"))], "trip").unwrap();

        assert_eq!(
            names_in(folder),
            ["IMG.jpg", "IMG.xmp", "trip.CR2", "trip.xmp"]
        );
        assert_eq!(read(&folder.join("trip.xmp")), "xmp");
    }

    #[test]
    fn a_pair_renamed_apart_gives_each_photo_the_xmp() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(&folder.join("IMG.CR2"), "raw");
        write(&folder.join("IMG.jpg"), "jpeg");
        write(&folder.join("IMG.xmp"), "xmp");

        rename_files_on_disk(
            &[text(&folder.join("IMG.CR2")), text(&folder.join("IMG.jpg"))],
            "trip_{sequence}",
        )
        .unwrap();

        assert_eq!(
            names_in(folder),
            ["trip_1.CR2", "trip_1.xmp", "trip_2.jpg", "trip_2.xmp"]
        );
        assert_eq!(read(&folder.join("trip_2.xmp")), "xmp");
    }

    #[test]
    fn the_last_new_name_takes_the_xmp_unless_the_old_one_is_still_used() {
        let (a, b) = (PathBuf::from("a.xmp"), PathBuf::from("b.xmp"));
        assert_eq!(
            xmp_after_rename(&[a.clone(), a.clone(), b.clone()], false),
            [(a.clone(), Carry::Copy), (b.clone(), Carry::Move)]
        );
        assert_eq!(
            xmp_after_rename(&[a.clone(), b.clone()], true),
            [(a, Carry::Copy), (b, Carry::Copy)]
        );
        assert!(xmp_after_rename(&[], false).is_empty());
    }

    #[test]
    fn xmp_text_escapes_and_reads_back() {
        assert_eq!(xmp_text("R&D <b> \"q\""), "R&amp;D &lt;b&gt; &quot;q&quot;");
        assert_eq!(xmp_text_read("R&amp;D &lt;b&gt;"), "R&D <b>");
        assert_eq!(xmp_text_read("R&D"), "R&D");
        assert_eq!(xmp_text_read("$1 fee"), "$1 fee");
    }

    #[test]
    fn tags_with_xml_characters_and_dollars_survive_the_xmp() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG.jpg");
        write(&photo, "jpeg");
        let with_tags = |tags: &[&str]| ImageMetadata {
            tags: Some(tags.iter().map(|tag| tag.to_string()).collect()),
            ..ImageMetadata::default()
        };

        // The first write creates the subject list and the label; the second
        // replaces them, which is where a `$1` used to be expanded.
        sync_metadata_to_xmp(&photo, &with_tags(&["old", "color:red"]), true);
        sync_metadata_to_xmp(
            &photo,
            &with_tags(&["R&D", "$1 fee", "<b>", "color:r&d"]),
            true,
        );

        let xmp = read(&dir.path().join("IMG.xmp"));
        assert!(xmp.contains("<rdf:li>R&amp;D</rdf:li>"), "{xmp}");
        assert!(xmp.contains("<rdf:li>$1 fee</rdf:li>"), "{xmp}");
        assert!(xmp.contains("<rdf:li>&lt;b&gt;</rdf:li>"), "{xmp}");
        assert!(xmp.contains("<xmp:Label>R&amp;d</xmp:Label>"), "{xmp}");
        assert!(!xmp.contains("old"), "{xmp}");

        let mut read_back = ImageMetadata::default();
        assert!(sync_metadata_from_xmp(&photo, &mut read_back, false));
        let tags = read_back.tags.unwrap();
        for tag in ["R&D", "$1 fee", "<b>", "color:r&d"] {
            assert!(tags.iter().any(|t| t == tag), "{tag} missing from {tags:?}");
        }
    }

    /// A sidecar with one key this version cannot read (here, snapshots in an
    /// old shape) still has its edit: the thumbnail key must come from that
    /// edit, as the editor reads it, not from "no edit".
    #[test]
    fn a_thumbnail_key_reads_the_edit_of_a_sidecar_with_one_bad_key() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG.CR2");
        let sidecar = dir.path().join("IMG.CR2.agdata");
        fs::write(&photo, "raw").unwrap();
        let path = photo.to_string_lossy().to_string();

        fs::write(
            &sidecar,
            r#"{ "version": 1, "rating": 4, "adjustments": { "exposure": 0.7 },
                "agSnapshots": { "not": "a list" } }"#,
        )
        .unwrap();
        let with_bad_key = crate::file_management::get_cache_key_hash(&path);

        fs::write(
            &sidecar,
            r#"{ "version": 1, "rating": 4, "adjustments": { "exposure": 0.7 } }"#,
        )
        .unwrap();
        let clean = crate::file_management::get_cache_key_hash(&path);

        assert!(clean.is_some());
        assert_eq!(with_bad_key, clean);
    }

    /// Working out a thumbnail's key only looks: no rescue copy, no rewrite of
    /// a sidecar with oversized EXIF values, whatever the sidecar holds.
    #[test]
    fn a_thumbnail_key_writes_nothing_beside_the_photo() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG.CR2");
        let sidecar = dir.path().join("IMG.CR2.agdata");
        fs::write(&photo, "raw").unwrap();
        let path = photo.to_string_lossy().to_string();
        let long = "x".repeat(600);
        for text in [
            r#"{ "version": 1, "adjustments": {}, "agSnapshots": { "not": "a list" } }"#
                .to_string(),
            format!(r#"{{ "version": 1, "adjustments": {{}}, "exif": {{ "Note": "{long}" }} }}"#),
        ] {
            fs::write(&sidecar, &text).unwrap();
            crate::file_management::get_cache_key_hash(&path);
            assert_eq!(fs::read_to_string(&sidecar).unwrap(), text);
            assert_eq!(names_in(dir.path()), ["IMG.CR2", "IMG.CR2.agdata"]);
        }
    }

    /// Import with "delete after import": the XMP may hold another app's
    /// edits, so when it does not reach the destination nothing at the source
    /// is removed.
    #[test]
    fn an_import_whose_xmp_does_not_arrive_deletes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (card, library) = (dir.path().join("card"), dir.path().join("library"));
        let (photo, xmp) = (card.join("IMG.CR2"), card.join("IMG.xmp"));
        write(&photo, "raw");
        write(&xmp, "lightroom edits");
        let imported = library.join("IMG.CR2");
        write(&imported, "raw");
        // A folder where the XMP should land: its copy fails.
        fs::create_dir_all(library.join("IMG.xmp")).unwrap();

        let sidecar = card.join("IMG.CR2.agdata");
        assert!(finish_import(&photo, &sidecar, &imported, true).is_err());
        assert_eq!(read(&photo), "raw");
        assert_eq!(read(&xmp), "lightroom edits");
    }

    #[test]
    fn an_import_carries_the_xmp_and_keeps_the_source_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let (card, library) = (dir.path().join("card"), dir.path().join("library"));
        let (photo, xmp) = (card.join("IMG.CR2"), card.join("IMG.xmp"));
        write(&photo, "raw");
        write(&xmp, "lightroom edits");
        let imported = library.join("IMG.CR2");
        write(&imported, "raw");

        let sidecar = card.join("IMG.CR2.agdata");
        finish_import(&photo, &sidecar, &imported, false).unwrap();
        assert_eq!(read(&library.join("IMG.xmp")), "lightroom edits");
        assert!(photo.exists() && xmp.exists());
    }
}
