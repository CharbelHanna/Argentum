/**
 * The Crop tab's Lens Correction eye. Ours.
 *
 * Kept with the Edit panel's eyes in `sectionVisibility`, as `lens`. Closed,
 * no lens correction applies, the camera's own or a lens profile's, and the
 * pane's settings stay as they are for when it opens again
 * (image_processing.rs `is_lens_correction_shown`).
 */

import type { Adjustments } from '../utils/adjustments';

const LENS = 'lens';

export const isLensShown = (adjustments: Adjustments): boolean => adjustments.sectionVisibility?.[LENS] !== false;

export const withLensShown = (adjustments: Adjustments, shown: boolean): Adjustments => ({
  ...adjustments,
  sectionVisibility: { ...adjustments.sectionVisibility, [LENS]: shown },
});
