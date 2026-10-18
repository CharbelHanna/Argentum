/**
 * Which arrangement of today's panels the editor shows, and how the parts only
 * the Modern arrangement has were left. Ours.
 *
 * Classic is today's arrangement and the default. Modern arranges the same
 * window like Lightroom Classic; see argentum/modern/ModernWindow.tsx. Chosen in Settings >
 * General > Layout, read once at startup, and stored in
 * argentum-processing.json beside Compact panels through the same `ag`
 * commands (mods/editor_layout.rs), not in their settings.json.
 *
 * Everything else the arrangement remembers is today's own state, saved where
 * it always was: panel widths and folds (useUIStore's workspace and
 * uiVisibility), the adjustment sections' folds, and whether the histogram is
 * showing (isWaveformVisible, which also has its keyboard shortcut).
 */

import { create } from 'zustand';
import { Panel } from '../../components/ui/AppProperties';
import { ag } from '../ag';

/** Mirrors mods::editor_layout::EditorLayout. Hand-written: the app does not generate its types yet. */
export type EditorLayoutName = 'classic' | 'modern';

/** Mirrors mods::editor_layout::ModernTheme. Hand-written, as above. */
export type ModernThemeName = 'grey' | 'deep' | 'light';

/** Mirrors mods::editor_layout::LayoutPanels. Hand-written, as above. */
export interface LayoutPanels {
  /** The Navigator section is open. */
  navigatorOpen: boolean;
  /** Saved by an earlier form of the Modern layout, which had a tab panel; kept so the file still reads. */
  tab: string | null;
  tabWidthPx: number;
}

/** As the arrangement first opens: the Navigator showing, Presets beside the adjustments at 245 px. */
export const DEFAULT_PANELS: LayoutPanels = { navigatorOpen: true, tab: Panel.Presets, tabWidthPx: 245 };
/** How far the tab panel drags. */
export const MIN_TAB_WIDTH_PX = 224;
export const MAX_TAB_WIDTH_PX = 400;

/**
 * Panels are saved once a change settles, not on every step of a drag: each
 * save is a read and a write of the preferences file.
 */
let panelsSave: ReturnType<typeof setTimeout> | undefined;

interface EditorLayoutState {
  /** Null until the preference has been read. */
  layout: EditorLayoutName | null;
  panels: LayoutPanels;
  /** The Modern window's palette (Settings > General > Theme while Modern is chosen). */
  theme: ModernThemeName;
  load: () => Promise<void>;
  choose: (layout: EditorLayoutName) => Promise<void>;
  chooseTheme: (theme: ModernThemeName) => Promise<void>;
  changePanels: (change: Partial<LayoutPanels>) => void;
}

export const useEditorLayout = create<EditorLayoutState>((set, get) => ({
  layout: null,
  panels: DEFAULT_PANELS,
  theme: 'grey',

  load: async () => {
    // Unreadable is Classic, today's arrangement, and the panels as they first open.
    const [layout, panels, theme] = await Promise.all([
      ag<EditorLayoutName>('editor_layout').catch((): EditorLayoutName => 'classic'),
      ag<LayoutPanels | null>('editor_layout_panels').catch(() => null),
      ag<ModernThemeName>('modern_theme').catch((): ModernThemeName => 'grey'),
    ]);
    set({ layout, panels: panels ?? DEFAULT_PANELS, theme });
  },

  choose: async (layout) => {
    const previous = get().layout ?? 'classic';
    // Shown at once: a choice that lags a click reads as broken.
    set({ layout });
    try {
      await ag('set_editor_layout', { layout });
    } catch {
      // Not saved, so not chosen: put back what will be there next start.
      set({ layout: previous });
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

/** Whether the Modern arrangement is chosen. */
export const useModernLayout = () => useEditorLayout((s) => s.layout === 'modern');
