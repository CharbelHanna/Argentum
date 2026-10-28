- **RapidRAW's AI models download through the shared model downloader.** Subject selection,
  Foreground, Sky, Depth, Relight, Inpainting, AI noise reduction and Photo tagging no longer keep
  their own copy of the model addresses and checksums, their own models-folder lookup, downloader
  and SHA-256 check (`ai_processing.rs`, about 280 lines): each asks `mods/model_catalog.rs` and
  `mods/model_download.rs`. Same folder, same file names, same checksums, so nothing already
  downloaded is fetched again; the sky model saved under its old name (`skyseg-u2net.onnx`) is
  still renamed rather than fetched, now also by Settings > AI models' Download. A download is
  streamed to a `.part` file and kept only once its checksum matches, instead of being held in
  memory, and its progress shows in Settings > AI models. The test that read `ai_processing.rs`
  to catch drifting copies is gone with the copies.
