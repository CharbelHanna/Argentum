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
//! - [`Destinations`]: no file of one copy or move lands on a file already
//!   there or on another file of the same operation.
//! - [`same_folder_copy`]: the names of a photo duplicated in its own folder.
//! - [`paths_after_rename`]: the selection after a rename, one path per input.
//! - [`xmp_text`] and [`xmp_text_read`]: tag and label text as XMP stores it.
//! - [`move_to_trash`]: the only way a library delete removes a file.
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

/// Why a file operation stopped before writing anything, or why a delete did
/// not happen.
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
    /// The system trash refused. Nothing is deleted for good in its place.
    Trash(String),
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
            Self::Trash(reason) => write!(
                f,
                "Could not move to the trash: {reason}. Nothing was deleted permanently."
            ),
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

/// Moves `paths` to the system trash: the only way a library delete removes a
/// file. When the trash refuses (a drive without one, no permission), the
/// error comes back and nothing is deleted for good in its place.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn move_to_trash(paths: &[PathBuf]) -> Result<(), FileOpError> {
    trash::delete_all(paths).map_err(|error| FileOpError::Trash(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_management::{
        copy_files, find_all_associated_files, move_files_on_disk, rename_files_on_disk,
        sync_metadata_from_xmp, sync_metadata_to_xmp,
    };
    use crate::image_processing::ImageMetadata;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
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
}
