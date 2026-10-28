import { describe, expect, it } from 'vitest';
import { isLightPalette, useEditorLayout } from './editorLayout';

// The two files' text, as Vite reads it.
const sources = import.meta.glob(['../../App.tsx', '../../components/adjustments/Curves.tsx'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const source = (path: string) => sources[`../../${path}`];

describe('the light palette', () => {
  it('is Light only', () => {
    expect(isLightPalette('light')).toBe(true);
    expect(isLightPalette('grey')).toBe(false);
    expect(isLightPalette('deep')).toBe(false);
  });

  it('follows the palette chosen in Settings', () => {
    useEditorLayout.setState({ theme: 'light' });
    expect(isLightPalette(useEditorLayout.getState().theme)).toBe(true);
    useEditorLayout.setState({ theme: 'grey' });
    expect(isLightPalette(useEditorLayout.getState().theme)).toBe(false);
  });

  // The Classic theme can no longer be set from Settings, so anything still
  // reading it stays dark on the Light palette: toasts and the curve
  // histogram did.
  it.each(['App.tsx', 'components/adjustments/Curves.tsx'])('is what %s follows', (file) => {
    const text = source(file);
    expect(text).toContain('useLightPalette()');
    expect(text).not.toMatch(/Theme\.(Light|Snow|Arctic)/);
  });
});
