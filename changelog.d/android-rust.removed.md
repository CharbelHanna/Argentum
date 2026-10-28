- **The app's Android code.** `src-tauri/src/android_integration.rs` (644 lines of JNI for content
  URIs, MediaStore and the Android context) and every `cfg(target_os = "android")` branch it fed:
  Android settings defaults, gallery and Downloads export, embedded lens database, content-URI
  import of photos and LUTs and their LUT cache, the internal library root, and the mobile entry
  point. Two commands only Android called are gone with it: `resolve_android_content_uri_name` and
  `get_or_create_internal_library_root`. Windows, macOS and Linux run the same code as before.
