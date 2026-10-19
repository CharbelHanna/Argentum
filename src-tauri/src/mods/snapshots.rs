//! Snapshots: named states of a photo's edit. Ours.
//!
//! The model is RAWmakase's (src/app/snapshots.rs @ 7c3c81a, Lightroom's
//! panel): + keeps the photo as it is now under a name, a click puts it back
//! as a History step, and each one can be updated, renamed or deleted.
//! RAWmakase keeps them in its catalog; Argentum has none, so they live in the
//! photo's sidecar, as `agSnapshots` beside the edit (ImageMetadata in
//! image_processing.rs), not inside it. Inside, an undo would take a snapshot
//! back out, and every save of the edit rewrites it; beside it, the edit's own
//! save (save_metadata_and_update_thumbnail) reads the sidecar and keeps them.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    /// When it was made or last updated, in milliseconds since 1970.
    pub at: u64,
    /// The whole edit as it was.
    pub adjustments: Value,
}

/// The photo's snapshots, none if it has no sidecar or none were made.
pub fn read(sidecar: &Path) -> Vec<Snapshot> {
    crate::exif_processing::load_sidecar(sidecar)
        .ag_snapshots
        .unwrap_or_default()
}

/// Keep `list` as the photo's snapshots, leaving the rest of the sidecar as
/// it is. An empty list takes the field out.
pub fn write(sidecar: &Path, list: Vec<Snapshot>) -> Result<(), String> {
    let mut metadata = crate::exif_processing::load_sidecar(sidecar);
    metadata.ag_snapshots = (!list.is_empty()).then_some(list);
    let json = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    std::fs::write(sidecar, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(name: &str, exposure: f64) -> Snapshot {
        Snapshot {
            id: format!("id-{name}"),
            name: name.to_string(),
            at: 1_760_000_000_000,
            adjustments: json!({ "exposure": exposure }),
        }
    }

    #[test]
    fn snapshots_are_kept_beside_the_edit() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("DSC0001.ARW.rrdata");
        std::fs::write(
            &sidecar,
            r#"{ "version": 1, "rating": 4, "adjustments": { "exposure": 1.5 }, "tags": ["kept"] }"#,
        )
        .unwrap();

        assert!(read(&sidecar).is_empty());
        let list = vec![snapshot("First pass", 0.5), snapshot("Warm", 1.0)];
        write(&sidecar, list.clone()).unwrap();
        assert_eq!(read(&sidecar), list);

        // The edit and the rest of the sidecar are as they were.
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        assert_eq!(saved["adjustments"]["exposure"], json!(1.5));
        assert_eq!(saved["rating"], json!(4));
        assert_eq!(saved["tags"], json!(["kept"]));
        assert!(saved["adjustments"].get("agSnapshots").is_none());
    }

    #[test]
    fn deleting_the_last_one_takes_the_field_out() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("DSC0002.ARW.rrdata");
        write(&sidecar, vec![snapshot("Only", 0.0)]).unwrap();
        write(&sidecar, Vec::new()).unwrap();
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        assert!(saved.get("agSnapshots").is_none());
        assert!(read(&sidecar).is_empty());
    }
}
