import { describe, expect, it } from 'vitest';
import fixture from './record.fixture.json';
import { INITIAL_ADJUSTMENTS, INITIAL_MASK_CONTAINER, type Adjustments } from '../../utils/adjustments';
import { useEditorStore } from '../../store/useEditorStore';
import { historyNames } from '../layout/historyNames';
import { HISTORY_STEPS_KEPT, compact, expand, type History, type HistoryRecord } from './historyRecord';

type Change = (state: Adjustments) => Adjustments;

/** As a sidecar gives it back: through JSON. */
const saved = <T>(value: T): T => JSON.parse(JSON.stringify(value));

/** A History made as the editor makes one: each state the last one with a change. */
const historyOf = (changes: Change[], end?: number): History => {
  const states = [INITIAL_ADJUSTMENTS];
  for (const change of changes) states.push(change(states[states.length - 1]));
  return { states, end: end ?? states.length - 1, names: historyNames(states) };
};

const set =
  (values: Partial<Adjustments>): Change =>
  (s) => ({ ...s, ...values });
const withMask: Change = (s) => ({ ...s, masks: [...s.masks, { ...INITIAL_MASK_CONTAINER, id: 'm1' }] });
const maskOpacity =
  (opacity: number): Change =>
  (s) => ({ ...s, masks: s.masks.map((m) => ({ ...m, opacity })) });
const maskExposure =
  (exposure: number): Change =>
  (s) => ({ ...s, masks: s.masks.map((m) => ({ ...m, adjustments: { ...m.adjustments, exposure } })) });
const redHue =
  (hue: number): Change =>
  (s) => ({ ...s, hsl: { ...s.hsl, reds: { ...s.hsl.reds, hue } } });
const lumaPoint =
  (y: number): Change =>
  (s) => ({
    ...s,
    curves: {
      ...s.curves,
      luma: [
        { x: 0, y: 0 },
        { x: 128, y },
        { x: 255, y: 255 },
      ],
    },
  });

describe('a History as its sidecar keeps it', () => {
  it('comes back as it was: every state, the step it was at, the names', () => {
    const history = historyOf(
      [
        set({ exposure: 0.7 }),
        redHue(12),
        lumaPoint(140),
        set({ lutPath: 'film.cube' }),
        withMask,
        maskOpacity(50),
        maskExposure(0.3),
        set({ lutPath: undefined }),
      ],
      6,
    );
    const record = saved(compact(history)!);
    const current = saved(history.states[6]);

    const kept = expand(record, current)!;
    expect(kept.end).toBe(6);
    expect(kept.names).toEqual(history.names);
    expect(saved(kept.states)).toEqual(saved(history.states));
    // The state it lands on is the open edit itself: taking the History changes nothing shown.
    expect(kept.states[6]).toBe(current);
  });

  it('keeps each step as what it changed, which the Rust side reads as its own', () => {
    const base = { ...INITIAL_ADJUSTMENTS, exposure: 0, masks: [{ ...INITIAL_MASK_CONTAINER, id: 'm1' }] };
    const states = [base, set({ exposure: 1 })(base)];
    states.push(maskOpacity(50)(states[1]));
    expect(compact({ states, end: 0, names: historyNames(states) })).toEqual(fixture);
  });

  it('is not taken when the edit has changed since', () => {
    const history = historyOf([set({ exposure: 0.7 }), set({ contrast: 12 })]);
    const record = saved(compact(history)!);
    expect(expand(record, saved(set({ contrast: 20 })(history.states[2])))).toBeNull();
  });

  it('is not taken when it does not fit the edit', () => {
    const history = historyOf([withMask, maskOpacity(50), maskOpacity(20)], 1);
    const record = saved(compact(history)!);
    // The mask its undone steps change is gone.
    const current = saved({ ...history.states[1], masks: [] });
    expect(expand(record, current)).toBeNull();
    const pastTheEnd: HistoryRecord = { ...record, end: record.steps.length };
    expect(expand(pastTheEnd, saved(history.states[1]))).toBeNull();
  });

  it('keeps nothing for a History of one state', () => {
    expect(compact(historyOf([]))).toBeNull();
  });

  it('keeps the last 50 states, as many as the editor holds', () => {
    const exposures = Array.from({ length: 60 }, (_, i) => set({ exposure: (i + 1) / 100 }));

    const latest = historyOf(exposures);
    const record = saved(compact(latest)!);
    expect(record.steps).toHaveLength(HISTORY_STEPS_KEPT);
    expect(record.end).toBe(HISTORY_STEPS_KEPT - 1);
    const kept = expand(record, saved(latest.states[60]))!;
    expect(kept.states[0].exposure).toBe(0.11);
    expect(saved(kept.states)).toEqual(saved(latest.states.slice(11)));

    // Most steps undone: the oldest go only as far as the step it is at, then the newest undone ones.
    const early = saved(compact(historyOf(exposures, 5))!);
    expect(early.steps).toHaveLength(HISTORY_STEPS_KEPT);
    expect(early.end).toBe(0);

    const editor = useEditorStore.getState();
    editor.resetHistory(INITIAL_ADJUSTMENTS);
    exposures.forEach((change) => editor.pushHistory(change(useEditorStore.getState().history.at(-1)!)));
    expect(useEditorStore.getState().history).toHaveLength(HISTORY_STEPS_KEPT);
  });

  it('keeps a 50-step edit in a few kilobytes', () => {
    const steps: Change[] = [];
    const sliders = ['exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'vibrance', 'saturation'];
    for (let i = 0; i < 29; i++) steps.push(set({ [sliders[i % sliders.length]]: (i % 7) * 3 + 0.25 }));
    for (let i = 0; i < 6; i++) steps.push(redHue(i * 5), lumaPoint(130 + i));
    steps.push(withMask);
    for (let i = 0; i < 6; i++) steps.push(maskExposure(i / 10 + 0.1));
    steps.push(set({ temperature: 12 }));
    const history = historyOf(steps);
    expect(history.states).toHaveLength(HISTORY_STEPS_KEPT);

    const record = compact(history)!;
    const bytes = JSON.stringify(record, null, 2).length;
    const newMask = JSON.stringify(
      record.steps.find((step) => step.name === 'Added Mask'),
      null,
      2,
    ).length;
    const oneEdit = JSON.stringify(history.states.at(-1), null, 2).length;
    console.log(
      `${history.states.length} states: ${bytes} bytes, ${newMask} of them the new mask; the edit: ${oneEdit}`,
    );
    // Set from the first measurement, 2026-10-22: 11,441 bytes, 5,409 of them the
    // new mask's own settings, the rest 6,032 for 49 steps. Never to grow.
    expect(bytes - newMask).toBeLessThanOrEqual(6_032);
    // The new mask is kept once, in the step that added it; the steps after keep a number each.
    expect(record.steps.filter((step) => JSON.stringify(step).includes('"New Mask"'))).toHaveLength(1);
  });
});
