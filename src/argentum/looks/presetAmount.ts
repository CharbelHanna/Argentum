/**
 * A preset's Amount: how far the photo goes from where it was towards the
 * preset, 0 to 200%. Ours.
 *
 * The mixing is RapidRAW's (src/components/panel/right/PresetsPanel.tsx,
 * `mixAdjustments` and `evaluateCurveY`, @ 79c2a46b): numbers in a straight
 * line, curves point by point, nested settings key by key, anything else
 * switching as soon as the Amount is above 0. Two things are RAWmakase's rule
 * instead (crates/rawmakase-interop/src/presets/amount.rs @ 7c3c81a):
 *
 * - It starts from the photo as it was before the preset, not from the app's
 *   defaults, and touches only the settings the preset has. RapidRAW mixed
 *   from the defaults, and at 0% put every default back, which undid the whole
 *   edit, crop and masks included.
 * - A colour grading wheel's hue (0 to 360 degrees) takes the shorter way
 *   round. Every other hue in the edit is a shift either side of none, so it
 *   mixes in a straight line like any number.
 */

type Values = Record<string, unknown>;
type Point = { x: number; y: number };

function evaluateCurveY(curve: Point[], targetX: number): number {
  const len = curve.length;
  if (len === 1) return curve[0].y;
  if (targetX <= curve[0].x) return curve[0].y;
  if (targetX >= curve[len - 1].x) return curve[len - 1].y;
  for (let i = 0; i < len - 1; i++) {
    const p2 = curve[i + 1];
    if (targetX <= p2.x) {
      const p1 = curve[i];
      const range = p2.x - p1.x;
      return range === 0 ? p1.y : p1.y + ((targetX - p1.x) / range) * (p2.y - p1.y);
    }
  }
  return targetX;
}

const isCurve = (value: unknown): value is Point[] =>
  Array.isArray(value) && value.length > 0 && typeof value[0]?.x === 'number' && typeof value[0]?.y === 'number';

function mixCurves(from: Point[], to: Point[], fraction: number): Point[] {
  const xs = [...new Set([...from.map((p) => p.x), ...to.map((p) => p.x)])].sort((a, b) => a - b);
  return xs.map((x) => {
    const y = evaluateCurveY(from, x) + (evaluateCurveY(to, x) - evaluateCurveY(from, x)) * fraction;
    return { x, y: Math.min(255, Math.max(0, y)) };
  });
}

/** Degrees, the shorter way round. */
function mixHue(from: number, to: number, fraction: number): number {
  const delta = ((((to - from) % 360) + 540) % 360) - 180;
  return from + delta * fraction;
}

/** The adjustments object holding the colour grading wheels. */
const GRADING = 'colorGrading';

function mixValue(path: string[], from: unknown, to: unknown, fraction: number): unknown {
  if (typeof to === 'number') {
    if (typeof from !== 'number') return fraction > 0 ? to : from;
    const wheelHue = path[0] === GRADING && path[path.length - 1] === 'hue';
    return wheelHue ? mixHue(from, to, fraction) : from + (to - from) * fraction;
  }
  if (isCurve(to)) {
    return isCurve(from) ? mixCurves(from, to, fraction) : fraction > 0 ? to : from;
  }
  if (to !== null && typeof to === 'object' && !Array.isArray(to)) {
    return mixValues((from ?? {}) as Values, to as Values, fraction, path);
  }
  return fraction > 0 ? to : from;
}

function mixValues(before: Values, preset: Values, fraction: number, path: string[] = []): Values {
  const out: Values = {};
  for (const key of Object.keys(preset)) {
    out[key] = mixValue([...path, key], before[key], preset[key], fraction);
  }
  return out;
}

/**
 * The settings the preset has, at `amount` percent from `before` (the photo
 * before the preset) to the preset: 0 is `before`'s own values, 100 the
 * preset's, 200 twice as far.
 */
export function presetAt<T extends object>(before: T, preset: Partial<T>, amount: number): Partial<T> {
  return mixValues(before as Values, preset as Values, amount / 100) as Partial<T>;
}
