- **Focus stacking and panoramas no longer crash the app on an odd frame.** A frame with a NaN
  pixel (blank or broken) made focus stacking panic while sorting (`focus_stacking.rs`, two
  sorts now use `total_cmp`), and two panorama photos whose match could not be inverted
  (degenerate points) panicked instead of saying so: the panorama now stops with "Photos 1 and 2
  could not be placed against each other." (`panorama_stitching.rs`). Same pixels otherwise.
