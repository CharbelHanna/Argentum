# 26.43.3

A typical edit and settings file as 26.43.3 wrote them, reconstructed once and never
edited since. Synthetic: no photo was read.

The stored shapes were taken from the 26.43.3 tag: `app_settings.rs`, `ImageMetadata`
in `image_processing.rs`, `mods/snapshots.rs`, `mods/history.rs`,
`mask_generation.rs`, both `defaultAdjustments.json` and `utils/adjustments.ts` are
the same at the tag as at the commit the 26.43.4 fixture was first written from
(`git diff 26.43.3 c86f85a7 -- <those files>` is empty), so the edit is the tag's starting values (`git show
26.43.3:src/utils/defaultAdjustments.json`, and `src/argentum/`'s) with a few
sliders moved, a crop, white balance in kelvin, lens correction, three masks as the
Masks panel and the canvas leave them (the other shape's -10000 placeholders
included), a Snapshot and a History. The settings are 26.43.3's defaults with a
handful of choices made. Unlike the newest fixture, most values are at their
default: the shape most sidecars have.
