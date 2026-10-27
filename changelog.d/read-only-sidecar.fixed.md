- **A thumbnail no longer shows an edited photo as unedited.** The grid read a photo's sidecar on
  its own: if one key did not read (snapshots in an older shape, say), it took the whole photo as
  unedited with no rating, while the editor kept the edit. Thumbnails and their cache key now read
  the sidecar the way the editor does, key by key, and only look: nothing is copied or rewritten
  while browsing (`peek_sidecar` in `src-tauri/src/exif_processing.rs`, on `salvage::readable`).
  Affected photos redraw their thumbnail once.
