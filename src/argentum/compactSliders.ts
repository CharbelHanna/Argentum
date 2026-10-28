/**
 * The compact panels and their row spacing. Ours.
 * (Compact sliders until the tone curve joined.)
 *
 * The layout is all in compactSliders.css, keyed on one attribute on <html>,
 * and the row spacing is one CSS variable beside it. The panels are always
 * compact, as the Modern look draws them; the switch that turned it off went
 * with Classic. This reads the row spacing at startup, puts both on the page,
 * and changes the spacing when Settings > General > Panels moves it.
 *
 * Stored in argentum-processing.json beside our other preferences, through the
 * same `ag` commands, rather than in their settings.json.
 */

import { create } from 'zustand';
import { ag } from './ag';
import './compactSliders.css';

const ATTRIBUTE = 'data-ag-sliders';
const GAP_VARIABLE = '--ag-slider-gap';

/** Space under each row, in pixels, until chosen: 20px a row, Lightroom's. */
export const DEFAULT_GAP = 4;
/** The slider's end. Rust clamps to the same. */
export const MAX_GAP = 12;

function showGap(gap: number) {
  document.documentElement.style.setProperty(GAP_VARIABLE, `${gap}px`);
}

/**
 * Row spacing is saved once the slider settles, not on every step of a drag:
 * each save is a read and a write of the preferences file.
 */
let gapSave: ReturnType<typeof setTimeout> | undefined;

interface CompactSlidersState {
  gap: number;
  load: () => Promise<void>;
  chooseGap: (gap: number) => void;
}

export const useCompactSliders = create<CompactSlidersState>((set) => ({
  gap: DEFAULT_GAP,

  load: async () => {
    document.documentElement.setAttribute(ATTRIBUTE, 'compact');
    // Unreadable is the default spacing.
    const gap = await ag('compact_slider_gap').catch(() => DEFAULT_GAP);
    set({ gap });
    showGap(gap);
  },

  chooseGap: (gap) => {
    const clamped = Math.max(0, Math.min(MAX_GAP, Math.round(gap)));
    // Live, so the panels move as the slider does.
    set({ gap: clamped });
    showGap(clamped);
    clearTimeout(gapSave);
    gapSave = setTimeout(() => {
      // A failed save leaves the spacing as chosen for this session; snapping
      // it back mid-drag would be worse than the next start forgetting it.
      ag('set_compact_slider_gap', { gap: clamped }).catch(() => {});
    }, 400);
  },
}));
