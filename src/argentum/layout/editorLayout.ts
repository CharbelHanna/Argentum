/**
 * How the Modern window was left and how it is drawn: its own parts' folds,
 * sizes and places, its palette and its text size. Ours.
 *
 * The Modern window (argentum/modern/ModernWindow.tsx) is the only
 * arrangement; the Layout choice and Classic's arrangement of the window are
 * gone. Read once at startup and stored in argentum-processing.json through
 * the `ag` commands (mods/editor_layout.rs), not in their settings.json.
 *
 * Everything else the arrangement remembers is today's own state, saved where
 * it always was: panel widths and folds (useUIStore's workspace and
 * uiVisibility), the adjustment sections' folds, and whether the histogram is
 * showing (isWaveformVisible, which also has its keyboard shortcut).
 */

import { create } from 'zustand';
import { Panel } from '../../components/ui/AppProperties';
import { ag } from '../ag';
import type { LayoutPanels, ModernTheme as ModernThemeName, TextSize as TextSizeName } from '../ag.generated';
export type { ModernTheme as ModernThemeName, TextSize as TextSizeName, LayoutPanels } from '../ag.generated';

/** As the arrangement first opens: the Navigator and History open in the left column. */
export const DEFAULT_PANELS: LayoutPanels = {
  navigatorOpen: true,
  tab: Panel.Presets,
  tabWidthPx: 245,
  historyOpen: true,
  historyHeightPx: null,
  snapshotsHeightPx: null,
  navigatorFloating: false,
  navigatorAt: null,
  folded: [],
};

/**
 * Panels are saved once a change settles, not on every step of a drag: each
 * save is a read and a write of the preferences file.
 */
let panelsSave: ReturnType<typeof setTimeout> | undefined;

interface EditorLayoutState {
  panels: LayoutPanels;
  /** The palette (Settings > General > Theme). */
  theme: ModernThemeName;
  /** How large the panels' text is (Settings > General > Panels). */
  textSize: TextSizeName;
  load: () => Promise<void>;
  chooseTheme: (theme: ModernThemeName) => Promise<void>;
  chooseTextSize: (textSize: TextSizeName) => Promise<void>;
  changePanels: (change: Partial<LayoutPanels>) => void;
}

export const useEditorLayout = create<EditorLayoutState>((set, get) => ({
  panels: DEFAULT_PANELS,
  theme: 'grey',
  textSize: 'normal',

  load: async () => {
    // Unreadable is the panels as they first open, Grey and Normal.
    const [panels, theme, textSize] = await Promise.all([
      ag('editor_layout_panels').catch(() => null),
      ag('modern_theme').catch((): ModernThemeName => 'grey'),
      ag('text_size').catch((): TextSizeName => 'normal'),
    ]);
    set({ panels: panels ?? DEFAULT_PANELS, theme, textSize });
  },

  chooseTextSize: async (textSize) => {
    const previous = get().textSize;
    // Shown at once: a choice that lags a click reads as broken.
    set({ textSize });
    try {
      await ag('set_text_size', { size: textSize });
    } catch {
      // Not saved, so not chosen: put back what will be there next start.
      set({ textSize: previous });
    }
  },

  chooseTheme: async (theme) => {
    const previous = get().theme;
    set({ theme });
    try {
      await ag('set_modern_theme', { theme });
    } catch {
      set({ theme: previous });
    }
  },

  changePanels: (change) => {
    const panels = { ...get().panels, ...change };
    set({ panels });
    clearTimeout(panelsSave);
    panelsSave = setTimeout(() => {
      // A failed save leaves the panels as left for this session; the next
      // start opening them as before is the whole cost.
      ag('set_editor_layout_panels', { panels }).catch(() => {});
    }, 400);
  },
}));

/**
 * Whether the Modern window is in charge: always, now that it is the only
 * arrangement. Kept while FolderTree.tsx still holds Classic's branches, which
 * carry the albums section and go with the album decision (build order slice
 * 8), so the places that ask stay findable for that removal.
 */
export const useModernLayout = () => true;

/** Is this palette the light one? Toasts and the curve histogram follow it. */
export const isLightPalette = (theme: ModernThemeName): boolean => theme === 'light';

/** The chosen palette is Light (Settings > General > Theme). */
export const useLightPalette = (): boolean => useEditorLayout((s) => isLightPalette(s.theme));
