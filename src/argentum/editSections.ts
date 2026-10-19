/**
 * What each section of the Edit panel holds, as the panel arranges them. Ours.
 *
 * Their sections (utils/adjustments.ts `ADJUSTMENT_SECTIONS`) keep white
 * balance, Vibrance and Saturation in Color, and Clarity, Dehaze, Structure and
 * Centre in Details. The panel shows them in Basic instead, the corrections
 * most photos need, in the order they are made, so a section's reset, copy and
 * paste follow them there; its eye does too, in the renderer's section checks
 * (image_processing.rs). Profile holds how the RAW is read, before any slider:
 * the camera profile, the RAW rendering and the tone mapper.
 *
 * Derived from their lists, not written out again, so a key they add to a
 * section still arrives in it.
 */

import { ADJUSTMENT_SECTIONS, INITIAL_ADJUSTMENTS, type Adjustments } from '../utils/adjustments';

/** Moved into Basic from Color and Details. `whiteBalanceAuto` is ours (WhiteBalanceMenu.tsx). */
const INTO_BASIC = [
  'temperature',
  'tint',
  'whiteBalance',
  'whiteBalanceAuto',
  'vibrance',
  'saturation',
  'clarity',
  'dehaze',
  'structure',
  'centré',
];

/** The Profile section, ahead of the others. It has no eye: none of it can be turned off. */
export const PROFILE_KEYS = ['cameraProfile', 'rawToneRendering', 'rawToneCurve', 'rawToneCurveMode', 'toneMapper'];

/** The manual fringe sliders, in Crop > Lens now (CropPanel.tsx): reset there with the lens. */
const FRINGE_KEYS = ['chromaticAberrationRedCyan', 'chromaticAberrationBlueYellow'];

/** The LUT, a look now (argentum/looks): no section's reset, copy or eye touches it. */
const LUT_KEYS = ['lutData', 'lutIntensity', 'lutName', 'lutPath', 'lutSize', 'lutIsSceneReferred'];

const without = (keys: string[], gone: string[]) => keys.filter((key) => !gone.includes(key));

export const EDIT_SECTION_KEYS: Record<string, string[]> = {
  basic: [...without(ADJUSTMENT_SECTIONS.basic, PROFILE_KEYS), ...INTO_BASIC],
  curves: ADJUSTMENT_SECTIONS.curves,
  color: without(ADJUSTMENT_SECTIONS.color, INTO_BASIC),
  // agSharpen is ours (Sharpening.tsx): Detail's reset, copy and paste take it with their sharpening.
  details: [...without(ADJUSTMENT_SECTIONS.details, [...INTO_BASIC, ...FRINGE_KEYS]), 'agSharpen'],
  effects: without(ADJUSTMENT_SECTIONS.effects, LUT_KEYS),
};

/** Their tools that left their section, for Basic, Crop or the Looks tab; the Customise menu no longer lists them. */
export const MOVED_TOOLS = ['whiteBalance', 'colorPresence', 'presence', 'lut', 'chromaticAberration'];

/** What `keys` start as on a new photo; a key with no starting value of theirs is cleared. */
export function initialValues(keys: string[]): Partial<Adjustments> {
  const values: Record<string, unknown> = {};
  for (const key of keys) {
    const initial = INITIAL_ADJUSTMENTS[key as keyof Adjustments];
    values[key] = initial === undefined ? undefined : structuredClone(initial);
  }
  return values as Partial<Adjustments>;
}
