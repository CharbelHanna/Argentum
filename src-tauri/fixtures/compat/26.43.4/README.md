# 26.43.4

Written by `write_this_release` in `src-tauri/src/mods/compat_fixture.rs` from the
code of 26.43.4 while it was unreleased (`c86f85a7`): every setting and every key of
the editor's starting values set to a valid value other than its default, a mask of
each kind the Masks panel makes, an AI patch of each kind, three Snapshots and a
History. Synthetic: no photo was read.

One change since, from the tag (`git diff c86f85a7 26.43.4 -- src-tauri/src/app_settings.rs`):
the lens in `myLenses` has `correctNew`, which 26.43.4 added before it shipped. No
other stored shape moved between the two (`ImageMetadata`, both
`defaultAdjustments.json`, `utils/adjustments.ts`, masks, Snapshots, History).

26.43.4 has shipped: this folder is never edited again.
