- **A photo's sidecar name, virtual-copy path and file name are each defined once.** In Rust,
  `file_management.rs` holds `sidecar_for` (`IMG.CR2.agdata`, `IMG.CR2.a1b2c3.agdata`),
  `split_virtual_path` and the `VIRTUAL_COPY` and `SIDECAR_EXTENSION` constants; every place
  that built a sidecar name or split `?vc=` by hand uses them, `get_primary_sidecar_path` is gone,
  and the thumbnail folder has one resolver instead of two. In the interface, `argentum/paths.ts`
  holds `fileNameOf`, `physicalPathOf`, `isVirtualCopyPath` and `VIRTUAL_COPY`, which replace 24
  hand-written `?vc=` splits and 18 copies of "the text after the last slash" across 22 files.
  The filmstrip now shows the name of a photo whose file name contains `?` in full; it cut the
  name at the first `?`.
