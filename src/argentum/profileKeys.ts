/**
 * Argentum's keys in a photo's edit for how the RAW is read: the camera
 * profile and the RAW rendering with its curve. Ours; their
 * `normalizeLoadedAdjustments` (utils/adjustments.ts) reads them back from a
 * saved edit through `loadedProfile`, one line in their file.
 *
 * Copy settings and presets carry `cameraProfile` and `rawToneRendering` (their
 * ADJUSTMENT_GROUPS, and mods/copy_keys.rs for the paste list), never
 * `rawToneCurve`: that curve is fitted to one photo. A photo that receives a
 * rendering fits its own, and `rawToneCurveMode` says which rendering a curve
 * was fitted for, so one left from another rendering is not used
 * (mods/raw_tone.rs `points_of`).
 */

import type { Coord } from '../utils/adjustments';

export const RAW_TONE_RENDERINGS = ['default', 'baseCurve', 'autoMatched', 'dngStandard'] as const;
export type RawToneRendering = (typeof RAW_TONE_RENDERINGS)[number];

interface ProfileValues {
  cameraProfile: string | null;
  rawToneRendering: RawToneRendering;
  rawToneCurve: Coord[] | null;
}

/** What a saved edit holds for them, each checked; the starting values for anything else. */
export function loadedProfile(
  loaded: Record<string, unknown>,
  initial: Pick<ProfileValues, 'rawToneRendering' | 'rawToneCurve'>,
): ProfileValues {
  const rendering = loaded.rawToneRendering as RawToneRendering;
  return {
    cameraProfile: typeof loaded.cameraProfile === 'string' ? loaded.cameraProfile : null,
    rawToneRendering: RAW_TONE_RENDERINGS.includes(rendering) ? rendering : initial.rawToneRendering,
    rawToneCurve: Array.isArray(loaded.rawToneCurve) ? (loaded.rawToneCurve as Coord[]) : initial.rawToneCurve,
  };
}
