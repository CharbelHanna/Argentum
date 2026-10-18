/**
 * Marks <html> for the Modern look, so modern.css applies, with its palette
 * and its text size. Ours.
 *
 *   data-ag-look="modern"                the Modern look, always
 *   data-ag-theme="grey|deep|light"      its palette (Settings > General > Theme)
 *   data-ag-text="small|normal|large"    its text size (Settings > General > Panels)
 *
 * The panels are always compact (compactSliders.ts).
 */

import { useEffect } from 'react';
import { useEditorLayout } from '../layout/editorLayout';
import './modern.css';

/** Called once, from Argentum.tsx. */
export function useModernLook() {
  const theme = useEditorLayout((s) => s.theme);
  const textSize = useEditorLayout((s) => s.textSize);

  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute('data-ag-look', 'modern');
    root.setAttribute('data-ag-theme', theme);
    root.setAttribute('data-ag-text', textSize);
  }, [theme, textSize]);
}
