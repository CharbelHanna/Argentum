- **Four unused pieces of code.** darktable's sigmoid tone curve (`mods/sigmoid.rs`), ported
  and then reverted, which nothing called; the hand-run survey of clipped highlights in
  `mods/highlights.rs`, which measured photos before highlight recovery was built and has no
  other use; the internal command `list_camera_profiles`, which the interface never called; and
  `src/i18n/update_translations.py`, a script for strings of the removed cloud features that
  nothing runs. No change in the app.
