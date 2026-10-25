// The camera lens keys copied wherever geometry leaves the edit (builtinKeys in
// argentum/lensBuiltin.ts): AI masks and Show Original see the preview's correction.
import { describe, expect, it } from 'vitest';
import { builtinKeys } from './lensBuiltin';
import type { Adjustments } from '../utils/adjustments';

describe('the camera lens keys geometry copies', () => {
  it('are the switch and the tables, and nothing else', () => {
    const tables = { source: 'DNG', default_on: true, vignetting: null, distortion: null, chromatic: null };
    const edit = { exposure: 1, crop: null, lensBuiltin: true, lensBuiltinParams: tables } as unknown as Adjustments;
    expect(builtinKeys(edit)).toEqual({ lensBuiltin: true, lensBuiltinParams: tables });
  });

  it('are empty for a photo without them', () => {
    expect(builtinKeys({ exposure: 1 } as unknown as Adjustments)).toEqual({
      lensBuiltin: undefined,
      lensBuiltinParams: undefined,
    });
  });
});
