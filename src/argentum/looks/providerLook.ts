/**
 * A provider look in a photo's edit. Ours.
 *
 * A LUT made for one photo, kept beside it (mods/provider_look.rs). `lutPath`
 * holds a marker rather than a path, and `agProviderLook` the camera, the look
 * and the rendering the look replaced. The marker is the switch: a look counts
 * only while `lutPath` is exactly the marker, so anything that writes a real
 * `lutPath` turns it off.
 *
 * A look goes on with Linear (its LUT is made for that), so whatever takes it
 * off also gives the photo its own rendering back, from `before`, which belongs
 * to that photo: it never travels to another.
 */

import type { Adjustments } from '../../utils/adjustments';
import type { ProviderLook, RenderingBefore } from '../ag.generated';

/** As `MARKER` in mods/provider_look.rs. */
export const PROVIDER_LOOK_PATH = 'provider-look';

/** As `KEY` in mods/provider_look.rs. */
export const PROVIDER_LOOK_KEY = 'agProviderLook';

type Edit = Partial<Adjustments>;

/** The rendering a look goes on with; the renderer fills in the photo's curve (mods/raw_tone.rs). */
const LINEAR: RenderingBefore = { rawToneRendering: 'linear', rawToneCurve: null, rawToneCurveMode: null };

const lookOf = (edit: Edit): ProviderLook | null =>
  edit.lutPath === PROVIDER_LOOK_PATH ? (edit.agProviderLook ?? null) : null;

const renderingOf = (edit: Edit): RenderingBefore => ({
  rawToneRendering: edit.rawToneRendering ?? null,
  rawToneCurve: edit.rawToneCurve ?? null,
  rawToneCurveMode: edit.rawToneCurveMode ?? null,
});

/** The keys that take a photo's provider look off; none for a photo without one. */
export const providerLookOff = (photo: Edit): Edit => {
  const on = lookOf(photo);
  return on ? { ...on.before, agProviderLook: null } : {};
};

/** Whether two edits have the same provider look on, or both none. */
export const sameProviderLook = (a: Edit, b: Edit): boolean => {
  const [one, other] = [lookOf(a), lookOf(b)];
  return one?.camera === other?.camera && one?.look === other?.look;
};

/**
 * What `changes`, made elsewhere (a preset, a LUT), write on `photo`. A provider
 * look they bring keeps this photo's rendering as its `before`, the first one
 * if a look is already on. Any other LUT in them takes a look on the photo off.
 */
export function lookOnto(photo: Edit, changes: Edit): Edit {
  const incoming = lookOf(changes);
  if (incoming) {
    const before = lookOf(photo)?.before ?? renderingOf(photo);
    return { ...changes, ...LINEAR, agProviderLook: { ...incoming, before } };
  }
  return changes.lutPath === undefined ? changes : { ...providerLookOff(photo), ...changes };
}
