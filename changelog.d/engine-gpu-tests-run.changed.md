- **The engine's GPU tests run in CI.** `engine.yml` installed its software GPU (lavapipe) after
  `cargo test`, so every GPU test there, the parity tests that hold the engine's copies of the
  app's shaders to the app's among them, said it skipped and passed. The software GPU now comes
  first, and with `AG_REQUIRE_GPU` set a test that finds no GPU fails instead of skipping
  (`src-tauri/engine/tests/common/mod.rs`). The memory budget, which had a step of its own for
  this, runs with the rest.
