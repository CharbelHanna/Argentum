- **One place names the models folder.** The selection masks and the graphics card runtime found
  `models` inside the app's data folder each in their own way; they now ask
  `mods/model_manager.rs` like every other AI feature (same folder as before). The catalogue's
  unused copy of the graphics card runtime's download address is gone, and the test that read
  `gpu_runtime.rs` to compare it is replaced by one that fails if any other file names the
  models folder itself.
