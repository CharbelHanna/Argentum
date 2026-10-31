/**
 * Provider looks: what a local program offers (mods/lut_provider.rs), and the
 * looks on their way from it. Ours.
 *
 * Rust only makes files. A look goes on here, as any look does, once its file
 * is ready: Linear and the LUT in one step, so one Undo (providerLook.ts). The
 * photo stays as it was until then, and may have been left by then.
 *
 * With no provider `offer` stays null: nothing shows, nothing listens, and what
 * is passed through here comes back as it was.
 */

import { createElement } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'react-toastify';
import { create } from 'zustand';
import { Invokes } from '../../components/ui/AppProperties';
import { useEditorStore } from '../../store/useEditorStore';
import { type Adjustments, getActiveTools, INITIAL_ADJUSTMENTS, setToolActive } from '../../utils/adjustments';
import { globalImageCache } from '../../utils/ImageLRUCache';
import { ag } from '../ag';
import type { LookProgress, OfferedLook, OfferedOption, ProviderLook, ProviderLooks } from '../ag.generated';
import { type AgKey, agText } from '../locales';
import {
  LOOK_KEYS,
  lookKeys,
  lookOf,
  lookOnto,
  type LookOptions,
  offeredOptions,
  sameOptions,
  withOption,
} from './providerLook';

/** As `EVENT` in mods/lut_provider.rs. */
const PROGRESS_EVENT = 'provider-look-progress';

/** How long a look can take before its row says so: a quick one never flickers. */
const SPIN_AFTER_MS = 300;

type Edit = Partial<Adjustments>;

interface Waiting {
  look: string;
  options: LookOptions;
  name: string;
  /** None until the queue answers. */
  job: number | null;
  spinning: boolean;
  /** `keep`: the look is in the edit already, only its file was missing or remade. */
  then: 'put' | 'keep';
  batch: boolean;
}

interface Note {
  photo: string;
  look: string;
  text: AgKey;
}

/** One photo's settings a batch changed, as they were. */
interface Before {
  path: string;
  values: Edit;
}

interface Batch {
  name: string;
  job: number | null;
  done: number;
  ended: number;
  total: number;
  before: Before[];
}

interface ProviderLooksState {
  offer: ProviderLooks | null;
  /** A hand choice of camera lasts for its photo. */
  chosen: Record<string, string>;
  /** So does one of options, until the look that has them is on it. */
  chosenOptions: Record<string, LookOptions>;
  /** By photo: a photo waits for one look. */
  waiting: Record<string, Waiting>;
  note: Note | null;
  batch: Batch | null;
  /** The last batch, for its Undo. */
  undoable: Before[];
  /** Counts the look files made, so what reads them reads again. */
  filesMade: number;
  /** What is offered, with the look files of `options`. */
  load: (photo: string | null, options?: LookOptions) => Promise<void>;
  reset: () => void;
  cameraOf: (photo: string, edit: Edit) => string | null;
  chooseCamera: (photo: string, camera: string, edit: Edit) => Promise<void>;
  optionsOf: (photo: string, edit: Edit) => LookOptions;
  chooseOption: (photo: string, option: OfferedOption, on: boolean, edit: Edit) => Promise<void>;
  pick: (photo: string, look: OfferedLook, camera: string, options: LookOptions) => Promise<void>;
  /** Files for a look already in these photos' edits, or a remake of one. */
  keepLook: (
    paths: string[],
    look: OfferedLook,
    camera: string,
    options: LookOptions,
    remake?: boolean,
  ) => Promise<void>;
  putOnSelection: (paths: string[], look: OfferedLook, camera: string | null, options: LookOptions) => Promise<void>;
  cancel: (photo: string) => Promise<void>;
  cancelBatch: () => Promise<void>;
  undoLast: () => Promise<void>;
  setOn: (on: boolean) => Promise<void>;
}

const FIRST = {
  offer: null,
  chosen: {},
  chosenOptions: {},
  waiting: {},
  note: null,
  batch: null,
  undoable: [],
  filesMade: 0,
};

/** A photo's missing file is asked for once a session: never in a loop. */
const asked = new Set<string>();
let listening = false;
let loads = 0;

const openPhoto = () => useEditorStore.getState().selectedImage?.path ?? null;

/** The change onto the open photo, as one step of its history. */
function changeOpenPhoto(change: (now: Adjustments) => Adjustments) {
  const { adjustments, pushHistory, setEditor } = useEditorStore.getState();
  const next = change(adjustments);
  setEditor({ adjustments: next });
  pushHistory(next);
}

export const useProviderLooks = create<ProviderLooksState>((set, get) => {
  const forget = (photos: string[]) =>
    set((state) => ({
      waiting: Object.fromEntries(Object.entries(state.waiting).filter(([photo]) => !photos.includes(photo))),
    }));

  const forgetChosenOptions = (photo: string) =>
    set((state) => ({
      chosenOptions: Object.fromEntries(Object.entries(state.chosenOptions).filter(([path]) => path !== photo)),
    }));

  /** Waits for `look` on `paths`; each answer comes through `settle`. */
  async function ask(
    paths: string[],
    camera: string | null,
    look: OfferedLook,
    options: LookOptions,
    how: { remake?: boolean; front: boolean; then: Waiting['then']; batch?: boolean },
  ) {
    const entry = {
      look: look.id,
      options,
      name: look.name,
      job: null,
      spinning: false,
      then: how.then,
      batch: !!how.batch,
    };
    const stillWanted = (path: string) => {
      const now = get().waiting[path];
      return now?.look === look.id && sameOptions(now.options, options);
    };
    set((state) => ({
      note: null,
      waiting: { ...state.waiting, ...Object.fromEntries(paths.map((path) => [path, entry])) },
    }));
    if (how.front) {
      const [photo] = paths;
      setTimeout(() => {
        if (stillWanted(photo))
          set((state) => ({ waiting: { ...state.waiting, [photo]: { ...state.waiting[photo], spinning: true } } }));
      }, SPIN_AFTER_MS);
    }
    let job: number;
    try {
      job = await ag('provider_look_make', {
        paths,
        camera,
        look: look.id,
        options,
        remake: how.remake ?? false,
        front: how.front,
      });
    } catch (err) {
      console.warn('Provider looks:', err);
      forget(paths);
      return;
    }
    const ours = paths.filter(stillWanted);
    if (ours.length === 0) {
      // Called off while the queue was answering.
      await ag('provider_look_cancel', { jobId: job }).catch(() => undefined);
      return;
    }
    set((state) => ({
      waiting: {
        ...state.waiting,
        ...Object.fromEntries(ours.map((path) => [path, { ...state.waiting[path], job }])),
      },
      batch: how.batch && state.batch ? { ...state.batch, job } : state.batch,
    }));
  }

  /** The look onto its photo, open or left, with what it changed kept for a batch. */
  async function putOn(progress: LookProgress, name: string, batch: boolean) {
    const keys = lookKeys(progress.camera, progress.look, name, progress.options);
    const keep = (was: Edit, own: Edit) => {
      if (!batch) return;
      const values = Object.fromEntries(Object.keys(own).map((k) => [k, was[k] ?? INITIAL_ADJUSTMENTS[k] ?? null]));
      set((state) => ({
        batch: state.batch && { ...state.batch, before: [...state.batch.before, { path: progress.path, values }] },
      }));
    };
    if (progress.path === openPhoto()) {
      changeOpenPhoto((now) => {
        const own = lookOnto(now, keys);
        keep(now, own);
        return { ...now, ...own, activeTools: setToolActive(getActiveTools(now), 'lut', true) };
      });
      return;
    }
    const meta = await invoke<{ adjustments?: Edit | null }>(Invokes.LoadMetadata, { path: progress.path }).catch(
      () => null,
    );
    const was = meta?.adjustments ?? {};
    const own = lookOnto(was, keys);
    keep(was, own);
    globalImageCache.delete(progress.path);
    await invoke(Invokes.ApplyAdjustmentsToPaths, { paths: [progress.path], adjustments: own });
  }

  /** The batch's note goes, and what went on can be undone, as for any look. */
  function endBatch() {
    const { batch } = get();
    if (!batch) return;
    set({ batch: null, undoable: batch.before });
    if (batch.before.length === 0) return;
    toast.success(
      createElement(
        'span',
        { className: 'flex items-center gap-3' },
        agText('looksApplied').replace('{name}', batch.name).replace('{count}', String(batch.before.length)),
        createElement('button', { className: 'underline', onClick: () => get().undoLast() }, agText('looksUndo')),
      ),
    );
  }

  async function settle(progress: LookProgress) {
    const waited = get().waiting[progress.path];
    if (waited?.look !== progress.look || !sameOptions(waited.options, progress.options)) return;
    if (waited.job !== null && waited.job !== progress.jobId) return;
    forget([progress.path]);
    const note = (text: AgKey) => set({ note: { photo: progress.path, look: progress.look, text } });
    if (progress.outcome === 'done') {
      set((state) => ({ filesMade: state.filesMade + 1 }));
      if (waited.then === 'put') {
        await putOn(progress, waited.name, waited.batch);
        // The edit has them now, so an Undo shows on the checkbox.
        if (!waited.batch) forgetChosenOptions(progress.path);
      } else if (progress.path === openPhoto()) {
        // The same edit, with its picture to draw again.
        const { adjustments, setEditor } = useEditorStore.getState();
        setEditor({ adjustments: { ...adjustments } });
      }
      if (progress.path === openPhoto()) {
        await get().load(progress.path, get().optionsOf(progress.path, useEditorStore.getState().adjustments));
      }
    } else if (progress.outcome === 'folder') note('looksProviderFolder');
    else if (progress.outcome !== 'cancelled') note('looksProviderFailed');
    if (!waited.batch) return;
    const { batch } = get();
    if (!batch) return;
    const counted = {
      ...batch,
      ended: batch.ended + 1,
      done: batch.done + (progress.outcome === 'done' ? 1 : 0),
    };
    set({ batch: counted });
    if (counted.ended >= counted.total) endBatch();
  }

  function startListening() {
    listening = true;
    listen<LookProgress>(PROGRESS_EVENT, (event) => settle(event.payload)).catch((err) =>
      console.warn('Provider looks:', err),
    );
  }

  async function cancelAll() {
    const jobs = new Set(Object.values(get().waiting).flatMap((w) => (w.job === null ? [] : [w.job])));
    set({ waiting: {} });
    await Promise.all([...jobs].map((jobId) => ag('provider_look_cancel', { jobId }).catch(() => undefined)));
  }

  return {
    ...FIRST,

    load: async (photo, options = {}) => {
      const mine = ++loads;
      const offer = await ag('provider_looks', { path: photo, options }).catch(() => null);
      if (mine !== loads) return;
      if (offer && !listening) startListening();
      set({ offer });
    },

    reset: () => {
      asked.clear();
      set(FIRST);
    },

    cameraOf: (photo, edit) => {
      const { offer, chosen } = get();
      if (chosen[photo]) return chosen[photo];
      const named = lookOf(edit)?.camera;
      if (named && offer?.cameras.some((c) => c.id === named)) return named;
      return offer?.photo?.camera ?? null;
    },

    chooseCamera: async (photo, camera, edit) => {
      await get().cancel(photo);
      set((state) => ({ chosen: { ...state.chosen, [photo]: camera }, note: null }));
      const on = lookOf(edit);
      const look = get().offer?.looks.find((l) => l.id === on?.look);
      if (look) await get().pick(photo, look, camera, get().optionsOf(photo, edit));
    },

    optionsOf: (photo, edit) => {
      const { chosenOptions, offer } = get();
      return chosenOptions[photo] ?? offeredOptions(offer?.options ?? [], lookOf(edit)?.options);
    },

    chooseOption: async (photo, option, on, edit) => {
      const options = withOption(get().optionsOf(photo, edit), option, on);
      await get().cancel(photo);
      set((state) => ({ chosenOptions: { ...state.chosenOptions, [photo]: options }, note: null }));
      const look = get().offer?.looks.find((l) => l.id === lookOf(edit)?.look);
      const camera = get().cameraOf(photo, edit);
      if (look && camera) await get().pick(photo, look, camera, options);
    },

    pick: (photo, look, camera, options) => ask([photo], camera, look, options, { front: true, then: 'put' }),

    keepLook: (paths, look, camera, options, remake = false) =>
      ask(paths, camera, look, options, { remake, front: paths.length === 1, then: 'keep' }),

    putOnSelection: async (paths, look, camera, options) => {
      set({ batch: { name: look.name, job: null, done: 0, ended: 0, total: paths.length, before: [] } });
      await ask(paths, camera, look, options, { front: false, then: 'put', batch: true });
    },

    cancel: async (photo) => {
      const waited = get().waiting[photo];
      if (!waited) return;
      forget([photo]);
      if (waited.job !== null) await ag('provider_look_cancel', { jobId: waited.job }).catch(() => undefined);
    },

    cancelBatch: async () => {
      await cancelAll();
      endBatch();
    },

    undoLast: async () => {
      const { undoable } = get();
      set({ undoable: [] });
      for (const { path, values } of undoable) {
        if (path === openPhoto()) changeOpenPhoto((now) => ({ ...now, ...values }));
        else {
          globalImageCache.delete(path);
          await invoke(Invokes.ApplyAdjustmentsToPaths, { paths: [path], adjustments: values });
        }
      }
      if (undoable.length) toast.info(agText('looksUndone'));
    },

    setOn: async (on) => {
      set((state) => ({ offer: state.offer && { ...state.offer, on }, note: null }));
      await cancelAll();
      endBatch();
      await ag('provider_looks_switch', { on }).catch((err) => console.warn('Provider looks:', err));
    },
  };
});

/** Whether the regular Presets and LUTs are the list that can't be used now. */
export const regularLooksAreOff = (state: Pick<ProviderLooksState, 'offer'>): boolean =>
  !!state.offer?.on && !!state.offer.photo;

/** The edit's provider look, or null. */
export const lookFor = (edit: Edit): ProviderLook | null => lookOf(edit);

/**
 * A paste carries the look, not a file, so each target asks for its own and it
 * goes on when that is ready. What comes back is what to paste now. The chosen
 * camera stays where it is valid; otherwise each photo gets its own (Rust).
 */
export function takeFromPaste(paths: string[], changes: Edit): Edit {
  const { offer, putOnSelection } = useProviderLooks.getState();
  const on = lookOf(changes);
  if (!offer || !on) return changes;
  const look = offer.looks.find((l) => l.id === on.look);
  if (look) void putOnSelection(paths, look, on.camera, offeredOptions(offer.options, on.options));
  return Object.fromEntries(Object.entries(changes).filter(([key]) => !LOOK_KEYS.includes(key)));
}

/** Its options, if the provider still offers them all: a file for others would not be this look's. */
function optionsToMake(offer: ProviderLooks, on: ProviderLook): LookOptions | null {
  const options = on.options ?? {};
  return sameOptions(offeredOptions(offer.options, options), options) ? options : null;
}

/**
 * The open photo's edit names a look with no file: it asks for it, when the
 * photo opens or a preset brings the look. This runs the program with no click,
 * so only for a camera, look and options it lists, and once a session.
 */
export async function restoreLook(photo: string, edit: Edit) {
  const { offer, keepLook } = useProviderLooks.getState();
  const on = lookOf(edit);
  if (!offer?.photo || !on) return;
  const options = optionsToMake(offer, on);
  const key = JSON.stringify([photo, on.camera, on.look, options]);
  const look = offer.looks.find((l) => l.id === on.look);
  if (!options || !look || !offer.cameras.some((c) => c.id === on.camera) || asked.has(key)) return;
  const there = await ag('provider_look_file', { path: photo, camera: on.camera, look: on.look, options }).catch(
    () => false,
  );
  if (there || asked.has(key)) return;
  asked.add(key);
  await keepLook([photo], look, on.camera, options);
}

/** A preset that brings a look, on photos in the grid: each asks for its file. */
export function restoreLooks(paths: string[], edit: Edit) {
  const { offer, keepLook } = useProviderLooks.getState();
  const on = lookOf(edit);
  const look = offer?.looks.find((l) => l.id === on?.look);
  const options = offer && on && optionsToMake(offer, on);
  if (on && look && options) void keepLook(paths, look, on.camera, options);
}
