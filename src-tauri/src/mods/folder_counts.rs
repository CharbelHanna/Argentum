//! How many photos a folder holds directly, not counting its subfolders, for
//! the numbers in the Modern folder column. Ours.
//!
//! What counts as a photo is today's folder tree's
//! (`formats::is_supported_image_file`, as RapidRAW
//! `src-tauri/src/file_management.rs` `scan_dir_lazy` uses it, @ 79c2a46b).
//! How much is read is not: theirs walks every folder below to add them up,
//! which on a whole disk is a scan of the disk. This reads the folder's own
//! listing once, the one the tree already reads to know whether the folder
//! opens, and never a file, so a photo kept in the cloud (OneDrive and the
//! like) is counted by its name and not downloaded. Totals with subfolders
//! need an index, which is Shoebox's job, not a scan.

use std::path::Path;

/// The photos directly in `folder`. A folder that cannot be read holds none.
pub fn photo_count(folder: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    let photos = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && crate::formats::is_supported_image_file(entry.path())
        })
        .count();
    u32::try_from(photos).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn photos_directly_inside_are_counted_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let day = dir.path().join("day 1");
        fs::create_dir_all(day.join("picks")).unwrap();
        fs::write(dir.path().join("a.dng"), b"").unwrap();
        fs::write(day.join("b.jpg"), b"").unwrap();
        fs::write(day.join("c.jpg"), b"").unwrap();
        fs::write(day.join("notes.txt"), b"").unwrap();
        fs::write(day.join("picks").join("d.jpg"), b"").unwrap();
        assert_eq!(photo_count(dir.path()), 1);
        assert_eq!(photo_count(&day), 2);
    }

    #[test]
    fn a_look_file_is_not_counted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.dng"), b"").unwrap();
        fs::write(dir.path().join("a.dng.cam1.abc.lut.png"), b"").unwrap();
        assert_eq!(photo_count(dir.path()), 1);
    }

    #[test]
    fn a_folder_that_is_not_there_holds_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(photo_count(&dir.path().join("gone")), 0);
    }
}
