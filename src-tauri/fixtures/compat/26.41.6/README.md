# 26.41.6

A settings file and an edit in the shape 26.41.6 wrote them, every key it had set to
a value other than its default; reconstructed once and never edited since.
Synthetic: no photo was read.

Made from the 26.43.4 fixture, cut back to what the 26.41.6 tag shows
(`git show 26.41.6:<file>`):

- settings: `app_settings.rs` had no `enableBaselineExposure`, `baselineExposure`
  or `compensateHighlightPreservation`; Copy settings' lists are that release's
  `all_available_adjustments`.
- the edit: the keys of that release's `INITIAL_ADJUSTMENTS` in `utils/adjustments.ts`
  (no starting-values JSON yet, no activeTools, AI denoise, baseline exposure,
  centre and vignette pins, relight, fog or B&W); RAW rendering `autoMatched`
  from its three, with no `rawToneCurveMode`; Looks up to 100%; clipping as a
  number, as its ControlsPanel cycled it. Masks as its Masks panel made them:
  section eyes in each mask's adjustments, no activeTools, Sharpen or Lightroom
  grading in them; Subject was `ai-subject`, drawn as a box; no Argentum
  selections. The AI patches are unchanged since (`AIPanel.tsx`, `maskUtils.ts`).
- the sidecar: `ImageMetadata` had no `agSnapshots` or `agHistory`.
