/**
 * What the Looks tab lists, and what it has put on the open photo. Ours.
 *
 * Three sources, read once and on demand: the presets that come with the app
 * (mods/looks.rs), the LUTs, the app's own film looks among them
 * (lut_processing.rs `list_luts`, `list_lut_libraries`), and the starred ids
 * (mods/looks.rs). The user's own presets come from their usePresets hook in
 * the panel itself.
 *
 * A preset put on the photo from here is remembered with what the photo was
 * before it, so its Amount can move between the two and taking it off puts
 * that back (presetAmount.ts). It belongs to the photo it went on: opening
 * another photo forgets it, as RapidRAW's own presets panel does.
 */

import { invoke } from '@tauri-apps/api/core';
import { create } from 'zustand';
import type { Adjustments } from '../../utils/adjustments';
import { ag } from '../ag';
import type { PresetGroup as BuiltinGroup } from '../ag.generated';
export type { BuiltinPreset, PresetGroup as BuiltinGroup } from '../ag.generated';

export interface LutEntry {
  name: string;
  path: string;
  isBuiltIn: boolean;
  libraryId: string | null;
}

export interface LutLibrary {
  id: string;
  name: string;
}

/** A preset put on the open photo from the Looks tab. */
export interface AppliedPreset {
  photo: string;
  id: string;
  name: string;
  adjustments: Partial<Adjustments>;
  /** The photo's settings just before the preset went on. */
  before: Adjustments;
  /** Percent, 0 to 200. */
  amount: number;
}

interface LooksState {
  builtin: BuiltinGroup[];
  luts: LutEntry[];
  libraries: LutLibrary[];
  favourites: string[];
  /** Whether `favourites` came from the file: until then a star is not written. */
  favouritesRead: boolean;
  applied: AppliedPreset | null;
  /** Save > As LUT file asked for the Export tab in CUBE; ExportPanel.tsx clears it. */
  wantLutExport: boolean;
  loaded: boolean;
  load: () => Promise<void>;
  refreshLuts: () => Promise<void>;
  toggleFavourite: (id: string) => void;
  setApplied: (applied: AppliedPreset | null) => void;
}

export const useLooks = create<LooksState>((set, get) => ({
  builtin: [],
  luts: [],
  libraries: [],
  favourites: [],
  favouritesRead: false,
  applied: null,
  wantLutExport: false,
  loaded: false,

  load: async () => {
    if (get().loaded) return;
    set({ loaded: true });
    const [builtin, favourites] = await Promise.all([
      ag('builtin_presets').catch(() => []),
      ag('looks_favourites').catch((err) => {
        console.error('Looks: could not read the starred looks:', err);
        return null;
      }),
      get().refreshLuts(),
    ]);
    set({ builtin, favourites: favourites ?? [], favouritesRead: favourites !== null });
  },

  refreshLuts: async () => {
    try {
      const [libraries, luts] = await Promise.all([
        invoke<LutLibrary[]>('list_lut_libraries'),
        invoke<LutEntry[]>('list_luts'),
      ]);
      set({ libraries, luts });
    } catch (err) {
      console.error('Looks: could not list the LUTs:', err);
    }
  },

  toggleFavourite: (id) => {
    // Unread, the list here is empty: writing it would replace the real one.
    if (!get().favouritesRead) return;
    const was = get().favourites;
    const favourites = was.includes(id) ? was.filter((f) => f !== id) : [...was, id];
    set({ favourites });
    ag('set_looks_favourites', { ids: favourites }).catch(() => set({ favourites: was }));
  },

  setApplied: (applied) => set({ applied }),
}));
