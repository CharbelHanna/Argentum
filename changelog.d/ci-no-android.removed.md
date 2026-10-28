- **The Android steps of the release build.** No row of the build matrix sets `mobile`, so
  `build.yml`'s Android steps (JDK, SDK, NDK, signing, the APK build and its uploads, about 100
  lines) never ran. They are gone, with the `mobile` input. The Android app itself
  (`src-tauri/gen/android`, `android_integration.rs`) is untouched until its own decision.
