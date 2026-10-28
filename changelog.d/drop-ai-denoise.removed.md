- **RapidRAW's live AI denoise base (`ai_denoise.rs`, 516 lines).** It was not taken at the 1.6.6
  release review: nothing generated its denoised base or set its cache folder, so it had nothing
  to blend and every photo loaded as it was, even an edit carrying its `aiDenoiseEnabled` flag
  (from a RapidRAW 1.6.6 preset). The image loader, export and thumbnails no longer ask it; their
  `prepare_base_image` was only `composite_patches_on_image` and is gone too. Denoise Image (the
  dialog) and its model are unchanged.
