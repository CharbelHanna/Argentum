- **Super Resolution and Restore download through the shared model downloader.** Their models
  (`realesrgan_x2plus.onnx`, `realesrgan_x4plus.onnx`, `omnilens_swinir_aodlib_256.onnx`) are now
  defined once, in `mods/model_catalog.rs`, and fetched by `mods/model_download.rs` like the Text,
  Face and selection masks: streamed to a `.part` file and moved into place once its SHA-256
  matches. Same folder, same file names, same checksums, so nothing already downloaded is fetched
  again; a test now holds the catalogue to every file an earlier version saved. Their own copy of
  the addresses, the folder lookup and the checksum code is gone. A download started this way now
  also shows its progress in Settings > AI models.
