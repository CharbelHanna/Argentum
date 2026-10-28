- **Argentum's own preferences share one shape.** Each one in `argentum-processing.json` is now a
  `Setting<T>` (`mods/ag_settings.rs`): its key and stored type, declared once by the module that
  owns it, read with `load` and written with `save`. The nine modules that paired a key string
  with their own JSON conversion (highlight recovery, raw decoder, TIFF depth, the Modern
  layout's panels, palette and text size, starred looks, recent folders, and two one-time
  markers) use it, and the compact slider gap moved from `commands.rs` beside the other layout
  preferences in `editor_layout.rs`. The file and every stored value read as before.
