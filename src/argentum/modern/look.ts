/**
 * Marks <html> while the Modern layout is chosen, so modern.css applies, and
 * draws the panels compact then. Ours.
 *
 *   data-ag-look="modern"           the Modern layout is chosen
 *   data-ag-theme="grey|deep|light" its palette (Settings > General > Theme)
 *
 * Classic sets neither and looks as it always has. Compact panels are forced
 * on while Modern shows: the one-line slider is part of its look, whatever the
 * Classic preference says (compactSliders.ts keeps that preference).
 */

import { useEffect } from 'react';
import { useCompactSliders } from '../compactSliders';
import { useEditorLayout } from '../layout/editorLayout';
import './modern.css';

/** Called once, from Argentum.tsx. */
export function useModernLook() {
  const modern = useEditorLayout((s) => s.layout === 'modern');
  const theme = useEditorLayout((s) => s.theme);

  useEffect(() => {
    const root = document.documentElement;
    if (modern) {
      root.setAttribute('data-ag-look', 'modern');
      root.setAttribute('data-ag-theme', theme);
    } else {
      root.removeAttribute('data-ag-look');
      root.removeAttribute('data-ag-theme');
    }
    useCompactSliders.getState().setForced(modern);
  }, [modern, theme]);
}
