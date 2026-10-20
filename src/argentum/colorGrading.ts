/**
 * Lightroom's colour grading, for the Color Grading panel. Ours.
 *
 * A Color Grading block (the photo's, or a mask's) renders with Lightroom's
 * measured grading when it carries `agLightroom: 1`
 * (src-tauri/src/mods/color_grade.rs). A block graded before that existed has
 * no mark and keeps RapidRAW's grading, so an existing edit keeps its look.
 * The Looks carry the mark already; this writes it the first time a block
 * that grades nothing is changed, so a new grading is Lightroom's.
 */

import type { Adjustments } from '../utils/adjustments';

type Block = Adjustments['colorGrading'];

/** The mark, as mods/color_grade.rs reads it: a number, as the block's type holds. */
const LIGHTROOM_KEY = 'agLightroom';

/** Whether any region of the block tints or changes luminance. */
const grades = (block: Block) =>
  [block.shadows, block.midtones, block.highlights, block.global].some(
    (region) => !!region && (region.saturation > 0 || region.luminance !== 0),
  );

/**
 * `next`, the block after a change to `before`: marked as Lightroom's when
 * `before` had no mark and graded nothing. A block graded before the mark
 * existed stays unmarked, and so keeps its look.
 */
export const markedGrading = (before: Block, next: Block): Block =>
  LIGHTROOM_KEY in before || grades(before) ? next : { ...next, [LIGHTROOM_KEY]: 1 };
