// An edited photo's corner mark tells a provider look from any other edit.

import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import EditMark from './EditMark';

describe('EditMark', () => {
  it('is the 35 mm roll on a photo with a provider look', () => {
    expect(renderToStaticMarkup(<EditMark hasProviderLook />)).toContain('lucide-film-roll');
  });

  it('is the sliders on any other edit', () => {
    expect(renderToStaticMarkup(<EditMark hasProviderLook={false} />)).toContain('lucide-sliders-horizontal');
  });
});
