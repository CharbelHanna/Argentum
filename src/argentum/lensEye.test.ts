// The Crop tab's Lens Correction eye: kept with the Edit panel's eyes, and
// opened again by a reset, as their sections' resets open theirs.

import { describe, expect, it } from 'vitest';
import type { Adjustments } from '../utils/adjustments';
import { isLensShown, withLensShown } from './lensEye';

const edit = (sectionVisibility?: Record<string, boolean>) => ({ sectionVisibility }) as unknown as Adjustments;

describe('the Lens Correction eye', () => {
  it('is open unless the edit says closed', () => {
    expect(isLensShown(edit())).toBe(true);
    expect(isLensShown(edit({ curves: false }))).toBe(true);
    expect(isLensShown(edit({ lens: false }))).toBe(false);
  });

  it('closes and opens without touching the other eyes', () => {
    const closed = withLensShown(edit({ curves: false }), false);
    expect(closed.sectionVisibility).toMatchObject({ curves: false, lens: false });
    expect(isLensShown(withLensShown(closed, true))).toBe(true);
    expect(withLensShown(closed, true).sectionVisibility).toMatchObject({ curves: false });
  });
});
