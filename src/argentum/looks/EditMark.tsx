/** An edited photo's corner mark: a 35 mm roll when a provider look is on. Ours. */

import { createLucideIcon, SlidersHorizontal } from 'lucide-react';

/** Lucide has none: can, spool and leader on its 24 px grid, so it sits beside its icons. */
const FilmRoll = createLucideIcon('film-roll', [
  ['path', { d: 'M4 2.5V1h4v1.5', key: 'spool' }],
  ['ellipse', { cx: '6', cy: '4.5', rx: '4.5', ry: '2', key: 'can-top' }],
  ['path', { d: 'M1.5 4.5V21c0 .8 2 2 4.5 2s4.5-1.2 4.5-2V4.5', key: 'can-side' }],
  ['path', { d: 'M10.5 6H22a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1h-6c-1.5 0-2 .8-2 2s-1 2.5-3.5 3', key: 'leader' }],
  ['path', { d: 'M14.5 9.5h.01', key: 'hole-1' }],
  ['path', { d: 'M19 9.5h.01', key: 'hole-2' }],
]);

export default function EditMark({ hasProviderLook }: { hasProviderLook: boolean }) {
  const Mark = hasProviderLook ? FilmRoll : SlidersHorizontal;
  return <Mark size={12} />;
}
