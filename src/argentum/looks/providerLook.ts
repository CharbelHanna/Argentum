/**
 * A provider look in a photo's edit. Ours.
 *
 * A LUT made for one photo, kept beside it (mods/provider_look.rs). `lutPath`
 * holds a marker rather than a path, and `agProviderLook` the camera, the look,
 * its options and the rendering the look replaced. The marker is the switch: a
 * look counts only while `lutPath` is exactly the marker, so anything that
 * writes a real `lutPath` turns it off.
 *
 * An edit holds only the options away from their default, so a look with all of
 * them at their default names the file it always did, and an edit saved before
 * options existed reads the same as one with every option at its default. What
 * an option means is the provider's: here it is an id, a label and a value.
 *
 * A look goes on with Linear (its LUT is made for that), so whatever takes it
 * off also gives the photo its own rendering back, from `before`, which belongs
 * to that photo: it never travels to another.
 */

import type { Adjustments } from '../../utils/adjustments';
import type { OfferedOption, ProviderLook, RenderingBefore } from '../ag.generated';

/** As `MARKER` in mods/provider_look.rs. */
export const PROVIDER_LOOK_PATH = 'provider-look';

/** As `KEY` in mods/provider_look.rs. */
export const PROVIDER_LOOK_KEY = 'agProviderLook';

type Edit = Partial<Adjustments>;

/** A look's options away from their default, by id. */
export type LookOptions = NonNullable<ProviderLook['options']>;

/** The rendering a look goes on with; the renderer fills in the photo's curve (mods/raw_tone.rs). */
const LINEAR: RenderingBefore = { rawToneRendering: 'linear', rawToneCurve: null, rawToneCurveMode: null };

/** The edit's provider look, or null. */
export const lookOf = (edit: Edit): ProviderLook | null =>
  edit.lutPath === PROVIDER_LOOK_PATH ? (edit.agProviderLook ?? null) : null;

/** The steps of a look's file: `GRID` in mods/lut_provider.rs. */
const LOOK_STEPS = 64;

/** Whether `option` is on, with `values` chosen. */
export const optionIsOn = (option: OfferedOption, values: LookOptions): boolean => values[option.id] ?? option.default;

/** `values` with `option` at `on`. */
export function withOption(values: LookOptions, option: OfferedOption, on: boolean): LookOptions {
  const others = Object.entries(values).filter(([id]) => id !== option.id);
  return Object.fromEntries(on === option.default ? others : [...others, [option.id, on]]);
}

/** The ones of `values` still among `offered`. */
export const offeredOptions = (offered: OfferedOption[], values: LookOptions = {}): LookOptions =>
  Object.fromEntries(Object.entries(values).filter(([id]) => offered.some((option) => option.id === id)));

export function sameOptions(a: LookOptions = {}, b: LookOptions = {}): boolean {
  const ids = Object.keys(a);
  return ids.length === Object.keys(b).length && ids.every((id) => a[id] === b[id]);
}

/** What a provider look writes on a photo, through `lookOnto`, which fits it to the photo. */
export const lookKeys = (camera: string, look: string, name: string, options: LookOptions = {}): Edit => ({
  lutPath: PROVIDER_LOOK_PATH,
  lutName: name,
  lutSize: LOOK_STEPS,
  lutIntensity: 100,
  lutIsSceneReferred: false,
  agLutDeclaredInput: true,
  agProviderLook: { camera, look, ...(Object.keys(options).length > 0 && { options }), before: LINEAR },
});

/** Every key a provider look writes on a photo. */
export const LOOK_KEYS = [...Object.keys(lookKeys('', '', '')), ...Object.keys(LINEAR), 'lutData'];

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
  return one?.camera === other?.camera && one?.look === other?.look && sameOptions(one?.options, other?.options);
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
