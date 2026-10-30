// An edited photo's corner mark tells a provider look from any other edit.

import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import EditMark from './EditMark';

// Every view that draws the mark, as Vite reads it.
const views = import.meta.glob(
  [
    '../../components/panel/Filmstrip.tsx',
    '../../components/panel/library/LibraryItems.tsx',
    '../../components/panel/library/CullingView.tsx',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

describe('EditMark', () => {
  it('is the 35 mm roll on a photo with a provider look', () => {
    expect(renderToStaticMarkup(<EditMark hasProviderLook />)).toContain('lucide-film-roll');
  });

  it('is the sliders on any other edit', () => {
    expect(renderToStaticMarkup(<EditMark hasProviderLook={false} />)).toContain('lucide-sliders-horizontal');
  });

  it.each(Object.keys(views))('is the only edit mark %s draws', (view) => {
    expect(views[view]).toContain('<EditMark hasProviderLook=');
    expect(views[view]).not.toContain('SlidersHorizontal');
  });
});
