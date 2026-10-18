/**
 * Where the Navigator sits in the Modern window: docked as the left column's
 * first section, or floating over the photo. Ours.
 *
 * Docked is where it starts. The button in its header floats it over the
 * photo, where its title strip drags it anywhere in the centre and it stays
 * where it was put; the same button there docks it again. Either way it shows
 * only while a photo is open. Both, and where it was put, are saved with the
 * layout (editorLayout.ts panels).
 */

import { type PointerEvent as ReactPointerEvent, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, PanelLeft, PictureInPicture2 } from 'lucide-react';
import Navigator, { type NavigatorProps } from '../layout/Navigator';
import { useEditorLayout } from '../layout/editorLayout';
import { useAgTranslation } from '../locales';

/** Floating, before it is first moved: the centre's bottom-right corner. */
const FIRST_PLACE: [number, number] = [1, 1];

export function DockedNavigator(props: NavigatorProps) {
  const t = useAgTranslation();
  const open = useEditorLayout((s) => s.panels.navigatorOpen);
  const changePanels = useEditorLayout((s) => s.changePanels);

  return (
    <div className="agm-navsec">
      <div className="agm-sec-head" onClick={() => changePanels({ navigatorOpen: !open })}>
        {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        {t('layoutNavigator')}
        <span className="agm-sec-tools">
          <button
            className="agm-btn"
            data-tooltip={t('mwNavFloat')}
            onClick={(e) => {
              e.stopPropagation();
              changePanels({ navigatorFloating: true });
            }}
          >
            <PictureInPicture2 size={13} />
          </button>
        </span>
      </div>
      {open && <Navigator {...props} />}
    </div>
  );
}

/**
 * Placed by fractions of the room it can move in, not of the centre: 0 is
 * against the left or top edge, 1 against the right or bottom, so it stays
 * inside however the window is resized.
 */
export function FloatingNavigator(props: NavigatorProps) {
  const t = useAgTranslation();
  const saved = useEditorLayout((s) => s.panels.navigatorAt);
  const changePanels = useEditorLayout((s) => s.changePanels);
  const box = useRef<HTMLDivElement>(null);
  const [moving, setMoving] = useState<[number, number] | null>(null);
  const [x, y] = moving ?? saved ?? FIRST_PLACE;

  const startMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const self = box.current;
    const room = self?.parentElement;
    if (!self || !room || e.button !== 0) return;
    e.preventDefault();
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    // On screen, not offsetLeft: the translate that keeps it inside moves it too.
    const mine = self.getBoundingClientRect();
    const area = room.getBoundingClientRect();
    const free = { w: area.width - mine.width, h: area.height - mine.height };
    const grab = { x: e.clientX - mine.left, y: e.clientY - mine.top };
    const fraction = (offset: number, span: number) => (span > 0 ? Math.min(1, Math.max(0, offset / span)) : 0);
    const at = (ev: PointerEvent): [number, number] => [
      fraction(ev.clientX - area.left - grab.x, free.w),
      fraction(ev.clientY - area.top - grab.y, free.h),
    ];
    let last: [number, number] | null = null;
    const move = (ev: PointerEvent) => {
      last = at(ev);
      setMoving(last);
    };
    const end = () => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', end);
      handle.removeEventListener('pointercancel', end);
      if (last) changePanels({ navigatorAt: last });
      setMoving(null);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', end);
    handle.addEventListener('pointercancel', end);
  };

  return (
    <div className="agm-navfloat-room">
      <div
        ref={box}
        className="agm-navfloat"
        style={{ left: `${x * 100}%`, top: `${y * 100}%`, transform: `translate(${-x * 100}%, ${-y * 100}%)` }}
      >
        <div className="agm-navfloat-head" onPointerDown={startMove}>
          {t('layoutNavigator')}
          <button
            className="agm-btn"
            data-tooltip={t('mwNavDock')}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => changePanels({ navigatorFloating: false, navigatorOpen: true })}
          >
            <PanelLeft size={13} />
          </button>
        </div>
        <Navigator {...props} />
      </div>
    </div>
  );
}
