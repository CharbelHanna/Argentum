/**
 * Auto white balance that follows a photo's settings to other photos. Ours.
 *
 * Auto measures the photo it is pressed on (WhiteBalanceMenu.tsx) and keeps
 * the answer as numbers, so on another photo those numbers would be the first
 * photo's light, not its own. So the answer, `whiteBalanceAuto`, names the
 * photo it was measured on, and wherever it arrives on a different one, that
 * photo is measured again:
 *
 * - After a paste, for every photo pasted onto that is not open
 *   (`settleAutoWhiteBalance`, mods/auto_wb_photo.rs), and after Looks puts a
 *   preset on the grid's selection.
 * - On the open photo, whenever its edit holds an Auto measured elsewhere: a
 *   paste or preset onto it, or a photo a paste missed, the next time it opens.
 *
 * An Auto from before the photo was named is taken as this photo's own.
 */

import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { useEffect } from 'react';
import { Invokes } from '../components/ui/AppProperties';
import { useEditorActions } from '../hooks/useEditorActions';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useProcessStore } from '../store/useProcessStore';
import { useSettingsStore } from '../store/useSettingsStore';
import {
  getWhiteBalanceMode,
  resolveWhiteBalance,
  toRelativeWhiteBalance,
  type WhiteBalance,
  WhiteBalanceMode,
  withKelvinWhiteBalance,
  withRelativeWhiteBalance,
} from '../utils/whiteBalance';
import { ag } from './ag';
import { agText } from './locales';

/** What Auto found, and the photo it was measured on. */
export type AutoMarker = WhiteBalance & { photo?: string };

/** The keys Auto writes into an edit. */
export interface AutoChanges {
  temperature: number;
  tint: number;
  whiteBalance: WhiteBalance | null;
  whiteBalanceAuto: AutoMarker;
}

/**
 * The edit for a white balance Auto found on `photo`, written the way their
 * picker writes its own: in kelvin in their Kelvin mode, otherwise as the
 * change from the camera's as-shot white balance.
 */
export function autoChanges(asShot: WhiteBalance, found: WhiteBalance, photo: string): AutoChanges {
  const kelvin = getWhiteBalanceMode(useSettingsStore.getState().appSettings) === WhiteBalanceMode.Kelvin;
  const start = { temperature: 0, tint: 0, whiteBalance: null };
  const next = kelvin
    ? withKelvinWhiteBalance(start, found)
    : withRelativeWhiteBalance(start, toRelativeWhiteBalance(asShot, found));
  return {
    temperature: next.temperature,
    tint: next.tint,
    whiteBalance: next.whiteBalance ?? null,
    whiteBalanceAuto: { ...resolveWhiteBalance(asShot, next), photo },
  };
}

/** Was this Auto measured on another photo than `path`? */
export const measuredElsewhere = (marker: AutoMarker | undefined, path: string) =>
  !!marker?.photo && marker.photo !== path;

interface Measured {
  asShot: WhiteBalance;
  whiteBalance: WhiteBalance;
}

/**
 * Measure each photo and write its own Auto, one at a time, each as it is
 * done. `from` is the photo the pasted Auto was measured on: a photo is
 * measured only once the paste has reached its sidecar, so what is written
 * here lands after the paste rather than under it.
 */
export async function settleAutoWhiteBalance(paths: string[], from: string) {
  if (paths.length === 0) return;
  const note = toast.info(agText('autoWbMeasuring').replace('{count}', String(paths.length)), {
    autoClose: false,
  });
  let done = 0;
  for (const path of paths) {
    try {
      const measured = await ag<Measured>('measure_auto_white_balance', { path, from });
      await invoke(Invokes.ApplyAdjustmentsToPaths, {
        paths: [path],
        adjustments: autoChanges(measured.asShot, measured.whiteBalance, path),
      });
      done += 1;
    } catch (err) {
      console.warn(`Auto white balance: ${path} kept the pasted one:`, err);
    }
  }
  toast.dismiss(note);
  if (done > 0) toast.success(agText('autoWbMeasured').replace('{count}', String(done)));
}

/** Mounted once (Argentum.tsx): the paste watcher and the open photo's. */
export function useAutoWhiteBalanceFollows() {
  const { setAdjustments } = useEditorActions();

  // A paste. Their paste goes to the selection, else the open photo
  // (useEditorActions handlePasteAdjustments); the open one is left to the
  // watcher below, and the photo the Auto came from needs nothing.
  useEffect(
    () =>
      useProcessStore.subscribe((state, before) => {
        if (!state.isPasted || before.isPasted) return;
        const marker = (useEditorStore.getState().copiedAdjustments as { whiteBalanceAuto?: AutoMarker } | null)
          ?.whiteBalanceAuto;
        const included = useSettingsStore.getState().appSettings?.copyPasteSettings?.includedAdjustments ?? [];
        if (!marker?.photo || !included.includes('whiteBalanceAuto')) return;
        const open = useEditorStore.getState().selectedImage?.path;
        const { multiSelectedPaths } = useLibraryStore.getState();
        const targets = multiSelectedPaths.length > 0 ? multiSelectedPaths : open ? [open] : [];
        settleAutoWhiteBalance(
          targets.filter((path) => path !== open && path !== marker.photo),
          marker.photo,
        );
      }),
    [],
  );

  // The open photo, measured with the editor's own picture. One that cannot be
  // measured keeps the pasted Auto, and is not asked again for the same one.
  useEffect(() => {
    let measuring: string | null = null;
    const failed = new Set<string>();
    return useEditorStore.subscribe(async (state) => {
      const photo = state.selectedImage;
      const marker = (state.adjustments as { whiteBalanceAuto?: AutoMarker }).whiteBalanceAuto;
      if (!photo?.isReady || !photo.asShotWhiteBalance || !measuredElsewhere(marker, photo.path)) return;
      const attempt = `${photo.path}
${marker?.photo}`;
      if (measuring === photo.path || failed.has(attempt)) return;
      measuring = photo.path;
      try {
        const result = await ag<{ whiteBalance: WhiteBalance }>('detect_auto_white_balance', {
          jsAdjustments: state.adjustments,
          mode: 'surfaces',
        });
        // Another photo may be open by the time the answer arrives.
        if (useEditorStore.getState().selectedImage?.path !== photo.path) return;
        const changes = autoChanges(photo.asShotWhiteBalance, result.whiteBalance, photo.path);
        setAdjustments((prev: object) => ({ ...prev, ...changes }));
      } catch (err) {
        failed.add(attempt);
        console.warn('Auto white balance: the open photo kept the pasted one:', err);
      } finally {
        measuring = null;
      }
    });
  }, [setAdjustments]);
}
