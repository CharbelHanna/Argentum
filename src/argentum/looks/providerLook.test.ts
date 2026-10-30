import { describe, expect, it } from 'vitest';
import { COPYABLE_ADJUSTMENT_KEYS, ADJUSTMENT_GROUPS, type Adjustments } from '../../utils/adjustments';
import { EDIT_SECTION_KEYS } from '../editSections';
import { presetAt } from './presetAmount';
import { lookOnto, PROVIDER_LOOK_PATH, providerLookOff, sameProviderLook } from './providerLook';

const curve = [
  { x: 0, y: 0 },
  { x: 255, y: 255 },
];

/** A photo with a look on, which replaced a fitted Base Curve rendering. */
const withLook = (camera = 'cam1', look = 'abc'): Partial<Adjustments> => ({
  lutPath: PROVIDER_LOOK_PATH,
  lutName: 'Invented Stock',
  lutIntensity: 80,
  rawToneRendering: 'linear',
  rawToneCurve: null,
  rawToneCurveMode: null,
  agProviderLook: {
    camera,
    look,
    before: { rawToneRendering: 'baseCurve', rawToneCurve: curve, rawToneCurveMode: 'baseCurve' },
  },
});

const realLut = { lutPath: '/luts/film.cube', lutName: 'film.cube', lutIntensity: 100 };

describe('taking a provider look off', () => {
  it('restores the rendering it replaced and clears the look', () => {
    expect(providerLookOff(withLook())).toEqual({
      rawToneRendering: 'baseCurve',
      rawToneCurve: curve,
      rawToneCurveMode: 'baseCurve',
      agProviderLook: null,
    });
  });

  it('changes nothing on a photo with none', () => {
    expect(providerLookOff({ lutPath: '/luts/film.cube', rawToneRendering: 'linear' })).toEqual({});
    expect(providerLookOff({})).toEqual({});
  });

  it('leaves a look a real lutPath already replaced', () => {
    expect(providerLookOff({ ...withLook(), lutPath: '/luts/film.cube' })).toEqual({});
  });
});

describe('sameProviderLook', () => {
  it('compares camera and look', () => {
    expect(sameProviderLook(withLook('cam1', 'abc'), withLook('cam1', 'abc'))).toBe(true);
    expect(sameProviderLook(withLook('cam1', 'abc'), withLook('cam2', 'abc'))).toBe(false);
    expect(sameProviderLook(withLook('cam1', 'abc'), withLook('cam1', 'xyz'))).toBe(false);
  });

  it('counts a look only while the marker is on', () => {
    expect(sameProviderLook({ ...withLook(), lutPath: '/luts/film.cube' }, {})).toBe(true);
    expect(sameProviderLook(withLook(), {})).toBe(false);
  });
});

describe('a preset applied to a photo', () => {
  const unedited: Partial<Adjustments> = {
    rawToneRendering: 'dngStandard',
    rawToneCurve: null,
    rawToneCurveMode: null,
  };

  it('with a provider look gets the target photo’s rendering as its before', () => {
    const saved = withLook();
    const put = lookOnto({ ...unedited, rawToneRendering: 'default' }, saved);
    expect(put.agProviderLook).toEqual({
      camera: 'cam1',
      look: 'abc',
      before: { rawToneRendering: 'default', rawToneCurve: null, rawToneCurveMode: null },
    });
    expect(saved.agProviderLook?.before.rawToneRendering).toBe('baseCurve');
  });

  it('with a provider look keeps the first before on a photo that has one', () => {
    const put = lookOnto(withLook('cam1', 'abc'), withLook('cam2', 'xyz'));
    expect(put.agProviderLook).toEqual({
      camera: 'cam2',
      look: 'xyz',
      before: { rawToneRendering: 'baseCurve', rawToneCurve: curve, rawToneCurveMode: 'baseCurve' },
    });
  });

  it('with a provider look goes on in Linear, whatever carried it', () => {
    const fitted = { rawToneRendering: 'baseCurve', rawToneCurve: curve, rawToneCurveMode: null };
    const { lutPath, agProviderLook } = withLook();
    const put = lookOnto(fitted, { lutPath, agProviderLook });
    expect([put.rawToneRendering, put.rawToneCurve, put.rawToneCurveMode]).toEqual(['linear', null, null]);
  });

  it('with a real LUT turns a provider look off, rendering included', () => {
    expect(lookOnto(withLook(), realLut)).toEqual({
      rawToneRendering: 'baseCurve',
      rawToneCurve: curve,
      rawToneCurveMode: 'baseCurve',
      agProviderLook: null,
      ...realLut,
    });
  });

  it('saved with no LUT turns a provider look off, rendering included', () => {
    expect(lookOnto(withLook(), { lutPath: null }).rawToneRendering).toBe('baseCurve');
  });

  it('with a real LUT and its own rendering keeps the preset’s rendering', () => {
    const preset = { ...realLut, rawToneRendering: 'default' };
    expect(lookOnto(withLook(), preset).rawToneRendering).toBe('default');
  });

  it('without a LUT leaves a provider look on', () => {
    expect(lookOnto(withLook(), { exposure: 1 })).toEqual({ exposure: 1 });
  });

  it('fades a different provider look in with its Amount', () => {
    const before = { ...withLook('cam1', 'abc'), lutIntensity: 80 } as Adjustments;
    const preset = { ...withLook('cam1', 'xyz'), lutIntensity: 100 };
    expect(presetAt(before, preset, 50).lutIntensity).toBe(50);
  });

  it('keeps a same provider look’s strength in the Amount', () => {
    const before = { ...withLook('cam1', 'abc'), lutIntensity: 80 } as Adjustments;
    const preset = { ...withLook('cam1', 'abc'), lutIntensity: 100 };
    expect(presetAt(before, preset, 50).lutIntensity).toBe(90);
  });

  it('switches the look whole: never a mix of two looks at any Amount', () => {
    const before = { ...withLook('cam1', 'abc') } as Adjustments;
    const preset = withLook('cam2', 'xyz');
    expect(presetAt(before, preset, 0).agProviderLook).toEqual(before.agProviderLook);
    expect(presetAt(before, preset, 40).agProviderLook).toEqual(preset.agProviderLook);
  });
});

describe('agProviderLook in the edit’s key lists', () => {
  it('travels in the LUT copy group', () => {
    const group = Object.values(ADJUSTMENT_GROUPS)
      .flat()
      .find((g) => g.label === 'modals.copyPaste.groups.lut');
    expect(group?.keys).toContain('agProviderLook');
    expect(COPYABLE_ADJUSTMENT_KEYS).toContain('agProviderLook');
  });

  it('is in no section’s reset, copy or eye', () => {
    expect(Object.values(EDIT_SECTION_KEYS).flat()).not.toContain('agProviderLook');
  });
});
