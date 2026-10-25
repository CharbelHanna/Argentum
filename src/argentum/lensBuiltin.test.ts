// How an open photo's edit gets the camera's lens tables (filled in argentum/lensBuiltin.ts).
import { describe, expect, it } from 'vitest';
import { filled, type LensCorrection } from './lensBuiltin';
import type { Adjustments } from '../utils/adjustments';

const tables = (corner: number): LensCorrection => ({
  source: 'Fujifilm built-in',
  default_on: true,
  vignetting: { knots: [0, 1], values: [1, corner] },
  distortion: null,
  chromatic: null,
});
const asEdit = (keys: object) => ({ exposure: 0.5, ...keys }) as unknown as Adjustments;

describe('the camera tables an edit gets', () => {
  it('fills a photo with no edit yet, the switch at the photo’s default', () => {
    expect(filled(asEdit({}), tables(1.5), true)).toEqual(
      asEdit({ lensBuiltinParams: tables(1.5), lensBuiltin: true }),
    );
    expect(filled(asEdit({}), tables(1.5), false)).toMatchObject({ lensBuiltin: false });
  });

  it('keeps a stored switch, on or off', () => {
    expect(filled(asEdit({ lensBuiltin: false }), tables(1.5), true)).toMatchObject({ lensBuiltin: false });
    expect(filled(asEdit({ lensBuiltin: true }), tables(1.5), false)).toMatchObject({ lensBuiltin: true });
  });

  it('keeps the tables a switched-on edit was saved with when the file now reads differently', () => {
    const saved = asEdit({ lensBuiltin: true, lensBuiltinParams: tables(1.4) });
    expect(filled(saved, tables(1.5), true)).toBeNull();
  });

  it('gives a switched-off edit the file’s tables, so turning it on uses them', () => {
    const saved = asEdit({ lensBuiltin: false, lensBuiltinParams: tables(1.4) });
    expect(filled(saved, tables(1.5), true)).toMatchObject({ lensBuiltin: false, lensBuiltinParams: tables(1.5) });
  });

  it('changes nothing when the edit already holds the file’s tables', () => {
    expect(filled(asEdit({ lensBuiltin: false, lensBuiltinParams: tables(1.5) }), tables(1.5), true)).toBeNull();
    expect(filled(asEdit({ lensBuiltin: true, lensBuiltinParams: tables(1.5) }), tables(1.5), true)).toBeNull();
  });

  it('fills a switched-on edit that lost its tables', () => {
    expect(filled(asEdit({ lensBuiltin: true, lensBuiltinParams: null }), tables(1.5), false)).toMatchObject({
      lensBuiltin: true,
      lensBuiltinParams: tables(1.5),
    });
  });
});
