/**
 * Subject, Sky, Background and Object masks, from RAWmakase. Ours.
 *
 * Subject finds the photo's people and animals, Sky its sky, Background
 * everything but the Subject; Object is whatever is clicked: a click adds,
 * Alt-click leaves out, a dragged box says where. The models run in
 * mods/selection.rs, on RAWmakase's code (src-tauri/rm_selection). They
 * replace their Sky and their Object (SAM 1, one box) in the add-mask menus;
 * Foreground stays, and masks made with theirs keep working.
 *
 * A selection is stored as their Foreground mask stores its own (their Sky's
 * for Sky), with `agSelection` naming which, so it renders through their code
 * unchanged. The menu entries are types of ours in Masks.tsx (`ag-subject`,
 * `ag-sky`, `ag-background`) that last only until the mask is made.
 *
 * The models (271 MB) are fetched only when asked, after saying their size,
 * and never when they are already in the app's model folder.
 */

import { listen } from '@tauri-apps/api/event';
import { useCallback } from 'react';
import { toast } from 'react-toastify';
import { Mask, type SubMask } from '../../components/panel/right/Masks';
import { useEditorActions } from '../../hooks/useEditorActions';
import { useEditorStore } from '../../store/useEditorStore';
import { useProcessStore } from '../../store/useProcessStore';
import type { Adjustments } from '../../utils/adjustments';
import { ag } from '../ag';
import { agText } from '../locales';

export type SelectionKind = 'subject' | 'sky' | 'background' | 'object';

const KINDS: Partial<Record<Mask, SelectionKind>> = {
  [Mask.AgSubject]: 'subject',
  [Mask.AgSky]: 'sky',
  [Mask.AgBackground]: 'background',
  [Mask.AgObject]: 'object',
};

/** The selection a menu entry makes, if it is one of ours. */
export const selectionKind = (type: Mask): SelectionKind | null => KINDS[type] ?? null;

/** Which of theirs a selection is stored as. */
export const storedType = (kind: SelectionKind): Mask => (kind === 'sky' ? Mask.AiSky : Mask.AiForeground);

/** The menu entry a stored selection came from, for its icon and name. */
export function menuTypeOf(subMask: Pick<SubMask, 'type' | 'parameters'>): Mask {
  const kind = subMask.parameters?.agSelection as SelectionKind | undefined;
  const entry = Object.entries(KINDS).find(([, k]) => k === kind)?.[0] as Mask | undefined;
  return entry ?? subMask.type;
}

/** What a new selection sub-mask starts with, before the models have answered. */
export const startingParameters = (kind: SelectionKind) => ({
  agSelection: kind,
  maskDataBase64: null,
  grow: 0,
  feather: 0,
  ...(kind === 'object' ? { agClicks: [], agBounds: null } : {}),
});

/** An Object selection: the canvas takes clicks and boxes for it. */
export const isObjectSelection = (subMask?: Pick<SubMask, 'parameters'> | null): boolean =>
  subMask?.parameters?.agSelection === 'object';

/** A click on the photo, in the canvas's frame of the whole photo, as the Object box is. */
export interface Click {
  x: number;
  y: number;
  positive: boolean;
}

/** A dragged box, in the same frame. */
export interface Bounds {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
}

interface Status {
  installed: boolean;
  missingBytes: number;
  totalBytes: number;
}

interface Selection {
  maskDataBase64: string;
  rotation: number;
  flipHorizontal: boolean;
  flipVertical: boolean;
  orientationSteps: number;
  found: boolean;
}

const megabytes = (bytes: number) => `${Math.round(bytes / 1_000_000)} MB`;

/** Asks once, in a note with two buttons, before anything is fetched. */
function askToDownload(bytes: number): Promise<boolean> {
  return new Promise((resolve) => {
    const id = toast.info(
      ({ closeToast }) => (
        <div className="flex flex-col gap-2">
          <span>{agText('selDownloadAsk').replace('{size}', megabytes(bytes))}</span>
          <span className="flex gap-3">
            <button
              className="underline"
              onClick={() => {
                resolve(true);
                closeToast();
              }}
            >
              {agText('selDownload')}
            </button>
            <button
              className="underline opacity-70"
              onClick={() => {
                resolve(false);
                closeToast();
              }}
            >
              {agText('selNotNow')}
            </button>
          </span>
        </div>
      ),
      { autoClose: false, closeOnClick: false, onClose: () => resolve(false) },
    );
    void id;
  });
}

/** The models are there, or fetched now with the user's yes. */
async function ensureModels(): Promise<boolean> {
  const status = await ag<Status>('selection_models_status');
  if (status.installed) return true;
  if (!(await askToDownload(status.missingBytes))) return false;
  const note = toast.info(agText('selDownloading').replace('{percent}', '0'), { autoClose: false });
  const unlisten = await listen<{ done: number; total: number }>('ag-selection-download', (event) => {
    const percent = Math.floor((100 * event.payload.done) / Math.max(1, event.payload.total));
    toast.update(note, { render: agText('selDownloading').replace('{percent}', String(percent)) });
  });
  try {
    await ag('download_selection_models');
    toast.update(note, { render: agText('selDownloaded'), type: 'success', autoClose: 3000 });
    return true;
  } catch (err) {
    toast.update(note, { render: String(err), type: 'error', autoClose: 8000 });
    return false;
  } finally {
    unlisten();
  }
}

/** The edit without what the picture does not depend on, as their AI masks send it. */
function geometryOf(adjustments: Adjustments) {
  const { masks: _masks, aiPatches: _patches, ...rest } = adjustments as Adjustments & { aiPatches?: unknown };
  return rest;
}

function findSubMask(adjustments: Adjustments, id: string): SubMask | undefined {
  return adjustments.masks?.flatMap((c) => c.subMasks).find((sm: SubMask) => sm.id === id);
}

/** The latest Object request per sub-mask: an answer to an older click is dropped. */
const objectRequests = new Map<string, number>();

/**
 * One gesture on an Object selection (ImageCanvas.tsx): a click (start and
 * end the same) adds a point, or leaves it out with Alt; a drag replaces the
 * box. The gesture is kept on the sub-mask at once, one History step, and the
 * outline follows when the model answers.
 */
export async function refineObject(
  subMaskId: string,
  start: { x: number; y: number },
  end: { x: number; y: number },
  leaveOut: boolean,
  update: (id: string, data: Partial<SubMask>) => void,
) {
  const sm = findSubMask(useEditorStore.getState().adjustments, subMaskId);
  if (!sm) return;
  const before = sm.parameters ?? {};
  const clicks: Click[] = [...(before.agClicks ?? [])];
  let bounds: Bounds | null = before.agBounds ?? null;
  if (start.x === end.x && start.y === end.y) clicks.push({ x: start.x, y: start.y, positive: !leaveOut });
  else bounds = { startX: start.x, startY: start.y, endX: end.x, endY: end.y };
  update(subMaskId, { parameters: { ...before, agClicks: clicks, agBounds: bounds } });
  if (!(await ensureModels())) return;

  const request = (objectRequests.get(subMaskId) ?? 0) + 1;
  objectRequests.set(subMaskId, request);
  const { adjustments, patchesSentToBackend } = useEditorStore.getState();
  const tasks = useProcessStore.getState();
  tasks.registerAiTask(subMaskId, 'subject');
  try {
    const { found: _found, ...result } = await ag<Selection>('selection_object', {
      jsAdjustments: geometryOf(adjustments),
      clicks,
      bounds,
      rotation: adjustments.rotation,
      flipHorizontal: adjustments.flipHorizontal,
      flipVertical: adjustments.flipVertical,
      orientationSteps: adjustments.orientationSteps,
    });
    if (objectRequests.get(subMaskId) !== request) return;
    const latest = findSubMask(useEditorStore.getState().adjustments, subMaskId)?.parameters ?? {};
    patchesSentToBackend.delete(subMaskId);
    update(subMaskId, { parameters: { ...latest, ...result } });
  } catch (err) {
    toast.error(agText('selFailed').replace('{error}', String(err)));
  } finally {
    if (objectRequests.get(subMaskId) === request) tasks.unregisterAiTask(subMaskId);
  }
}

/** Clears an Object selection's clicks, box and outline, to begin again. */
export function startOverObject(subMaskId: string, update: (id: string, data: Partial<SubMask>) => void) {
  const sm = findSubMask(useEditorStore.getState().adjustments, subMaskId);
  if (!sm) return;
  objectRequests.set(subMaskId, (objectRequests.get(subMaskId) ?? 0) + 1);
  useEditorStore.getState().patchesSentToBackend.delete(subMaskId);
  update(subMaskId, { parameters: { ...sm.parameters, agClicks: [], agBounds: null, maskDataBase64: null } });
}

/** Fills a new selection sub-mask: MasksPanel calls it as theirs call their own. */
export function useSelectionMasks() {
  const { setAdjustments } = useEditorActions();
  const registerAiTask = useProcessStore((s) => s.registerAiTask);
  const unregisterAiTask = useProcessStore((s) => s.unregisterAiTask);

  const fill = useCallback(
    (subMaskId: string, parameters: object) =>
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        masks: prev.masks.map((c) => ({
          ...c,
          subMasks: c.subMasks.map((sm: SubMask) =>
            sm.id === subMaskId ? { ...sm, parameters: { ...sm.parameters, ...parameters } } : sm,
          ),
        })),
      })),
    [setAdjustments],
  );

  /** A selection that will not be made: its empty sub-mask goes, and its mask with it if it was alone. */
  const drop = useCallback(
    (subMaskId: string) =>
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        masks: prev.masks
          .map((c) => ({ ...c, subMasks: c.subMasks.filter((sm: SubMask) => sm.id !== subMaskId) }))
          .filter((c) => c.subMasks.length > 0),
      })),
    [setAdjustments],
  );

  return useCallback(
    async (subMaskId: string, kind: SelectionKind) => {
      if (!useEditorStore.getState().selectedImage?.path) return;
      if (!(await ensureModels())) {
        drop(subMaskId);
        return;
      }
      // Object waits for clicks on the photo (refineObject).
      if (kind === 'object') return;
      const { adjustments, patchesSentToBackend } = useEditorStore.getState();
      registerAiTask(subMaskId, 'foreground');
      try {
        const { found, ...parameters } = await ag<Selection>('selection_mask', {
          kind,
          jsAdjustments: geometryOf(adjustments),
          rotation: adjustments.rotation,
          flipHorizontal: adjustments.flipHorizontal,
          flipVertical: adjustments.flipVertical,
          orientationSteps: adjustments.orientationSteps,
        });
        patchesSentToBackend.delete(subMaskId);
        fill(subMaskId, parameters);
        if (!found) toast.info(agText(kind === 'sky' ? 'selNoSky' : 'selNoSubject'));
      } catch (err) {
        toast.error(agText('selFailed').replace('{error}', String(err)));
      } finally {
        unregisterAiTask(subMaskId);
      }
    },
    [drop, fill, registerAiTask, unregisterAiTask],
  );
}
