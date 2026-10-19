/**
 * One-click Subject, Sky and Background masks, from RAWmakase. Ours.
 *
 * Subject finds the photo's people and animals, Sky its sky, Background
 * everything but the Subject. The models run in mods/selection.rs, on
 * RAWmakase's code (src-tauri/rm_selection). They replace the old Sky in the
 * add-mask menus; Object (a box round anything) and Foreground stay.
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

export type SelectionKind = 'subject' | 'sky' | 'background';

const KINDS: Partial<Record<Mask, SelectionKind>> = {
  [Mask.AgSubject]: 'subject',
  [Mask.AgSky]: 'sky',
  [Mask.AgBackground]: 'background',
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
});

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

  return useCallback(
    async (subMaskId: string, kind: SelectionKind) => {
      if (!useEditorStore.getState().selectedImage?.path) return;
      if (!(await ensureModels())) return;
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
    [fill, registerAiTask, unregisterAiTask],
  );
}
