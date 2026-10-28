/**
 * A photo's History in the compact form its sidecar keeps (mods/history.rs). Ours.
 *
 * darktable's model (src/common/exif.cc @ f973fb0f): the steps, and the one the
 * edit is at, so steps undone before the photo was left can still be redone. Each
 * step keeps its name and only what it changed, each value under its place in the
 * edit as a JSON Pointer (RFC 6901): a slider is `"/exposure": 0.7`, a mask's
 * opacity `"/masks/0/opacity": 50`, never the masks again with their bitmaps.
 * Step 0 keeps what the first state had that the edit now has not.
 *
 * So a record is read against the edit beside it: `expand` replays it from that
 * edit and gives it back only if it lands there again. An edit changed since (a
 * paste, a reset, another app) gets null, and the editor starts a fresh History.
 */

import { normalizeLoadedAdjustments, type Adjustments } from '../../utils/adjustments';
import type { HistoryRecord, HistoryStep } from '../ag.generated';
export type { HistoryStep, HistoryRecord } from '../ag.generated';

/** A History as the editor holds it: every state, the one shown, and each one's name. */
export interface History {
  states: Adjustments[];
  end: number;
  names: string[];
}

/** States kept, the first one included: as many as the editor keeps (pushHistory in useEditorStore.ts). */
export const HISTORY_STEPS_KEPT = 50;

type Node = Record<string, unknown>;

const isObject = (value: unknown): value is Node =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** Keys with a value: JSON drops the others, so they are not there once saved. */
const keysOf = (node: Node) => Object.keys(node).filter((key) => node[key] !== undefined);

/** Equal as JSON keeps them. */
const sameValue = (a: unknown, b: unknown): boolean => {
  if (a === b) return true;
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((v, i) => sameValue(v, b[i]));
  if (!isObject(a) || !isObject(b)) return false;
  const keys = keysOf(a);
  return keys.length === keysOf(b).length && keys.every((key) => sameValue(a[key], b[key]));
};

/** The same edit, once each is read the way a saved one is: a missing value and its default are one. */
export const sameEdit = (a: Adjustments, b: Adjustments) =>
  sameValue(normalizeLoadedAdjustments(a), normalizeLoadedAdjustments(b));

/** A place in the edit as a JSON Pointer, and back. */
const pointer = (path: string[]) => path.map((key) => `/${key.replace(/~/g, '~0').replace(/\//g, '~1')}`).join('');
const pathOf = (place: string): string[] =>
  place === ''
    ? []
    : place
        .slice(1)
        .split('/')
        .map((key) => key.replace(/~1/g, '/').replace(/~0/g, '~'));

/** What turns `from` into `to`, added to `step`. Arrays of one length are compared item by item. */
const collect = (from: unknown, to: unknown, path: string[], step: Required<Omit<HistoryStep, 'name'>>) => {
  if (from === to) return;
  if (isObject(from) && isObject(to)) {
    for (const key of keysOf(to)) {
      if (from[key] === undefined) step.set[pointer([...path, key])] = to[key];
      else collect(from[key], to[key], [...path, key], step);
    }
    for (const key of keysOf(from)) if (to[key] === undefined) step.unset.push(pointer([...path, key]));
  } else if (Array.isArray(from) && Array.isArray(to) && from.length === to.length) {
    to.forEach((item, i) => collect(from[i], item, [...path, String(i)], step));
  } else if (!sameValue(from, to)) {
    step.set[pointer(path)] = to;
  }
};

const changes = (name: string, from: Adjustments, to: Adjustments): HistoryStep => {
  const step: Required<Omit<HistoryStep, 'name'>> = { set: {}, unset: [] };
  collect(from, to, [], step);
  return {
    name,
    ...(Object.keys(step.set).length > 0 && { set: step.set }),
    ...(step.unset.length > 0 && { unset: step.unset }),
  };
};

/** A step that does not fit the edit it is replayed on. */
class Unfit extends Error {}

/** `node`'s `key`, which must be one it can have: a step only goes where its diff found something. */
const child = (node: unknown, key: string): unknown => {
  const fits = Array.isArray(node) ? /^(0|[1-9]\d*)$/.test(key) && Number(key) < node.length : isObject(node);
  if (!fits) throw new Unfit();
  return (node as Node)[key];
};

const copyOf = (node: unknown): Node => (Array.isArray(node) ? [...node] : { ...(node as Node) }) as Node;

/** `node` with `value` at `path`, sharing everything off the path. */
const setAt = (node: unknown, [key, ...rest]: string[], value: unknown): unknown => {
  if (key === undefined) return value;
  const inner = child(node, key);
  const copy = copyOf(node);
  copy[key] = setAt(inner, rest, value);
  return copy;
};

/** `node` without the key at `path`. */
const unsetAt = (node: unknown, [key, ...rest]: string[]): unknown => {
  if (key === undefined) throw new Unfit();
  const inner = child(node, key);
  const copy = copyOf(node);
  if (rest.length > 0) copy[key] = unsetAt(inner, rest);
  else if (isObject(node)) delete copy[key];
  else throw new Unfit();
  return copy;
};

const replay = (state: Adjustments, step: HistoryStep): Adjustments => {
  let next: unknown = state;
  for (const [place, value] of Object.entries(step.set ?? {})) next = setAt(next, pathOf(place), value);
  for (const place of step.unset ?? []) next = unsetAt(next, pathOf(place));
  if (!isObject(next)) throw new Unfit();
  return next as Adjustments;
};

/** The last HISTORY_STEPS_KEPT states around `end`: the oldest go first, then the newest undone ones. */
const capped = ({ states, end, names }: History): History => {
  const first = Math.min(end, Math.max(0, states.length - HISTORY_STEPS_KEPT));
  const last = first + HISTORY_STEPS_KEPT;
  return { states: states.slice(first, last), end: end - first, names: names.slice(first, last) };
};

/** `history` as its sidecar keeps it, against the edit it is at. None for a History of one state. */
export function compact(history: History): HistoryRecord | null {
  const { states, end, names } = capped(history);
  if (states.length < 2) return null;
  return {
    end,
    steps: states.map((state, i) => changes(names[i], i === 0 ? states[end] : states[i - 1], state)),
  };
}

/**
 * The History `record` keeps, replayed from `current`, the edit open now. Null
 * when it does not land on that edit, or does not fit it. The state it lands on
 * is `current` itself, so taking the History never changes what is shown.
 */
export function expand(record: HistoryRecord, current: Adjustments): History | null {
  const { steps, end } = record;
  if (steps.length < 2 || end >= steps.length) return null;
  const states: Adjustments[] = [];
  try {
    let state = current;
    for (const step of steps) {
      state = replay(state, step);
      states.push(state);
    }
  } catch (err) {
    if (err instanceof Unfit) return null;
    throw err;
  }
  if (!sameEdit(states[end], current)) return null;
  states[end] = current;
  return capped({ states, end, names: steps.map((step) => step.name) });
}
