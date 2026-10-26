// Which edits take the camera colour the DNG way (agDngColour, mods/dng_colour.rs), as the editor
// reads a photo's edit: a new edit does, one saved before does not. RapidRAW 3f5345a5, ours.
import { describe, expect, it } from 'vitest';
import { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments, type Adjustments } from '../utils/adjustments';

const loaded = (edit: Record<string, unknown>) => normalizeLoadedAdjustments(edit as unknown as Adjustments);

describe('the camera colour an edit takes', () => {
  it('is the DNG way for a new edit', () => {
    expect(INITIAL_ADJUSTMENTS.agDngColour).toBe(true);
  });

  it('stays rawler’s for an edit saved before', () => {
    expect(loaded({ exposure: 0.5, baselineExposure: true }).agDngColour).toBe(false);
  });

  it('is what the edit says once it says', () => {
    expect(loaded({ exposure: 0.5, agDngColour: true }).agDngColour).toBe(true);
    expect(loaded({ exposure: 0.5, agDngColour: false }).agDngColour).toBe(false);
  });

  it('is the DNG way for an edit that holds nothing, as the backend reads it', () => {
    expect(loaded({}).agDngColour).toBe(true);
  });
});
