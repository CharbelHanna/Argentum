- **The golden photos can fail a pull request.** The ten CC0 RAWs CI renders and holds to their
  references (`src-tauri/src/mods/look_check.rs`) ran with `continue-on-error`, so a photo whose look
  changed was reported and nothing stopped. Since the references were re-blessed on
  2026-10-26 they passed 26 runs in a row on CI's software GPU, 14 of them on main, so the step
  is now part of the required `App unit tests` check. A change meant to move a photo blesses it
  in the same pull request (`AG_LOOK_BLESS`, a row in `src-tauri/tests/look/LEDGER.tsv`). A sample
  that cannot be downloaded still skips the step rather than failing it.
