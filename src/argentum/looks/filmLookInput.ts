/**
 * A LUT as the Looks tab puts it on, with the mark that feeds a film look what
 * its file declares. Ours.
 *
 * The film looks that come with the app are made for V-Log over V-Gamut; the
 * renderer converts the picture to V-Gamut for an edit whose
 * `agLutDeclaredInput` is true (src-tauri/src/mods/film_lut.rs). A film look
 * put on before has no mark and keeps its look. Every LUT the Looks tab puts
 * on or previews carries the mark, whatever kind it is: a user's LUT goes
 * after the rendering, where the mark is never read.
 *
 * Installed at import: the mark joins their LUT copy group, so Copy settings,
 * Paste and a saved preset carry it with the LUT, and no line of theirs
 * changes. See the `film-look-input` entry in scripts/upstream-registry.mjs.
 */

import { ADJUSTMENT_GROUPS, COPYABLE_ADJUSTMENT_KEYS } from '../../utils/adjustments';

/** As `KEY` in mods/film_lut.rs. */
const DECLARED_INPUT_KEY = 'agLutDeclaredInput';

/**
 * The keys that name a LUT on a photo, as their handleLutSelect and
 * setLutPreviewOverride write them, with the mark.
 */
export const lutKeys = (path: string, isBuiltIn: boolean) => ({
  lutPath: path,
  lutName: path.split(/[\\/]/).pop() || 'LUT',
  lutIsSceneReferred: isBuiltIn,
  [DECLARED_INPUT_KEY]: true,
});

const lutGroup = Object.values(ADJUSTMENT_GROUPS)
  .flat()
  .find((group) => group.label === 'modals.copyPaste.groups.lut');
if (lutGroup && !lutGroup.keys.includes(DECLARED_INPUT_KEY)) {
  lutGroup.keys.push(DECLARED_INPUT_KEY);
  COPYABLE_ADJUSTMENT_KEYS.push(DECLARED_INPUT_KEY);
}
