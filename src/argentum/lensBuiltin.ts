/**
 * The camera's built-in lens correction, filled into the open photo's edit. Ours.
 *
 * Cameras store their lens's distortion, colour fringing and corner shading
 * in the RAW, for the focal length and aperture used: Fujifilm in every RAF,
 * Sony in an ARW, Nikon in a NEF or NRW, a DNG (Leica, DJI's drones, phones,
 * Lightroom's own) as opcodes. The engine's reader for each make says which
 * parts Lightroom applies by default, and so whether the switch starts on.
 * `lens_builtin` (mods/lens_builtin.rs) reads it from the file;
 * this puts it into the edit as `lensBuiltinParams`, with the switch,
 * `lensBuiltin`, at the photo's own default when the edit has none yet. From
 * there it travels like lensfun's numbers: preview, export, masks and the
 * thumbnail all read it from the edit.
 *
 * WHEN
 *
 * The file is asked once per photo per session, as soon as the photo is
 * chosen. The edit is filled the moment the photo is ready, which is before
 * its first render, so opening a photo saves nothing: the edit is only
 * written once something is changed. The same check runs on every change to
 * the editor while the photo is open, so a reset that drops the keys gets
 * them back, with the switch at its default.
 *
 * Not an edit, so not in the history: undoing past it fills it in again.
 */

import { useEffect } from 'react';
import { ag } from './ag';
import { useEditorStore } from '../store/useEditorStore';
import type { Adjustments } from '../utils/adjustments';

interface Radial {
  knots: number[];
  values: number[];
}

/**
 * HAND-WRITTEN STUB of `rm_lens_builtin::LensCorrection`
 * (src-tauri/engine/src/rm_lens_builtin.rs), until today's interface has
 * generated types. Only `source` and which parts are present are read here.
 */
export interface LensCorrection {
  source: string;
  default_on: boolean;
  vignetting: Radial | null;
  distortion: Radial | null;
  chromatic: [Radial, Radial] | null;
  /** The parts the camera always corrects; absent: all three. */
  always?: { distortion: boolean; chromatic: boolean; vignetting: boolean };
  /** The optical centre, in fractions of the image; absent: stored before it was read. */
  centre?: [number, number];
}

/** What `lens_builtin` answers. */
export interface Found {
  correction: LensCorrection | null;
  defaultOn: boolean;
  /** The photo has no saved edit (argentum/newPhotoDefaults.ts). */
  fresh: boolean;
}

/** The edit's keys for it (mods/lens_builtin.rs). */
interface BuiltinEdit {
  lensBuiltin?: boolean;
  lensBuiltinParams?: LensCorrection | null;
}

const edit = (adjustments: Adjustments) => adjustments as Adjustments & BuiltinEdit;

/** The photo's own tables, when its edit holds them. */
export const builtinTables = (adjustments: Adjustments): LensCorrection | null =>
  edit(adjustments).lensBuiltinParams ?? null;

/** The correction the edit applies: the photo's tables while the switch is on. */
export const builtinInUse = (adjustments: Adjustments): LensCorrection | null =>
  edit(adjustments).lensBuiltin === true ? builtinTables(adjustments) : null;

/** The edit with the switch set. */
export const withBuiltin = (adjustments: Adjustments, on: boolean): Adjustments =>
  ({ ...adjustments, lensBuiltin: on }) as Adjustments;

/**
 * The edit's two keys for it, to spread wherever the photo's geometry is
 * copied out of the edit (AI masks, Show Original), so those see the same
 * correction as the preview.
 */
export const builtinKeys = (adjustments: Adjustments): BuiltinEdit => {
  const { lensBuiltin, lensBuiltinParams } = edit(adjustments);
  return { lensBuiltin, lensBuiltinParams };
};

/** Spread into a reset: the switch goes back to the photo's own default. */
export const LENS_BUILTIN_RESET = { lensBuiltin: undefined } as Partial<Adjustments>;

/** Per path, for the session: a file's tables do not change while the app runs. */
const asked = new Map<string, Promise<Found>>();

/** What `lens_builtin` answers for `path`, asked once per session. */
export const ask = (path: string): Promise<Found> => {
  let answer = asked.get(path);
  if (!answer) {
    answer = ag<Found>('lens_builtin', { path });
    asked.set(path, answer);
    answer.catch(() => asked.delete(path));
  }
  return answer;
};

/**
 * The edit with the file's tables filled in, or `null` when it needs nothing.
 * An edit whose switch is on keeps the tables it was saved with, so a newer
 * reader never moves a photo already corrected, and the preview stays what
 * an export of the saved edit renders. Any other edit takes the file's.
 */
export const filled = (
  adjustments: Adjustments,
  correction: LensCorrection,
  defaultOn: boolean,
): Adjustments | null => {
  const current = edit(adjustments);
  if (current.lensBuiltin === true && current.lensBuiltinParams) return null;
  const same = JSON.stringify(current.lensBuiltinParams ?? null) === JSON.stringify(correction);
  if (same && current.lensBuiltin !== undefined) return null;
  return {
    ...adjustments,
    lensBuiltinParams: correction,
    lensBuiltin: current.lensBuiltin ?? defaultOn,
  } as Adjustments;
};

/** Fills the open photo's edit in, once it is this photo's and when it lacks either key. */
const fillFor = (path: string, found: Found & { correction: LensCorrection }) => {
  // The tables last found to need nothing, so most changes compare a reference.
  let checked: unknown = null;
  return () => {
    const { selectedImage, adjustments, setEditor } = useEditorStore.getState();
    // isReady: until then the adjustments may still be the last photo's.
    if (selectedImage?.path !== path || !selectedImage.isReady) return;
    const current = edit(adjustments);
    if (current.lensBuiltin !== undefined && current.lensBuiltinParams === checked) return;
    const next = filled(adjustments, found.correction, found.defaultOn);
    if (!next) {
      checked = current.lensBuiltinParams;
      return;
    }
    checked = found.correction;
    setEditor({ adjustments: next });
  };
};

/** Mounted once, in Argentum.tsx. */
export function useLensBuiltinOnOpen() {
  const path = useEditorStore((s) => s.selectedImage?.path);

  useEffect(() => {
    if (!path) return;
    let stopped = false;
    let unsubscribe = () => {};
    ask(path)
      .then((found) => {
        if (stopped || !found.correction) return;
        const fill = fillFor(path, { ...found, correction: found.correction });
        fill();
        unsubscribe = useEditorStore.subscribe(fill);
      })
      .catch((err) => console.error('Built-in lens correction could not be read:', path, err));
    return () => {
      stopped = true;
      unsubscribe();
    };
  }, [path]);
}

/** The lensfun choice's look while the camera's correction replaces it. */
export const LENSFUN_UNUSED = 'opacity-50 grayscale pointer-events-none';
