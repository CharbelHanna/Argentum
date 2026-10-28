- **The Android project inherited from RapidRAW.** No Argentum build has ever targeted Android,
  and nothing compiled it since the fork. `src-tauri/gen/android` (42 files), the Android-only
  crates in `src-tauri/Cargo.toml` (`ndk-context`, `jni`, `rustls-platform-verifier`,
  `include_dir`) and `build.rs`'s download of ONNX Runtime for Android are gone. Desktop builds are
  unchanged. The tag `android-last` marks the last commit that still has all of it.
