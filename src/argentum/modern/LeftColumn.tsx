/**
 * The Modern window's left column: the Navigator while a photo is open, where
 * the photos are (Folders) and when the edits happened (History), plus the
 * camera while tethered shooting is available. Ours.
 *
 * Folders is today's folder tree in its Modern form (FolderTree.tsx: Favourites,
 * Recent, This computer). History is today's undo steps (layout/HistoryPanel);
 * the line above it drags to share the column with Folders, and a double-click
 * on it puts it back. The column is resizable from its right edge; double-click
 * resets it. What is folded and History's height are saved with the layout.
 */

import { type ReactNode, type PointerEvent as ReactPointerEvent, useRef, useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Orientation, Panel } from '../../components/ui/AppProperties';
import Resizer from '../../components/ui/Resizer';
import { useUIStore } from '../../store/useUIStore';
import HistoryPanel from '../layout/HistoryPanel';
import { useEditorLayout } from '../layout/editorLayout';
import { useAgTranslation } from '../locales';

/** History's height until it is dragged, and the least either part keeps. */
const HISTORY_START_PX = 220;
const HISTORY_MIN_PX = 72;
const FOLDERS_MIN_PX = 140;

interface LeftColumnProps {
  width: number;
  /** The docked Navigator, while a photo is open and it is not floating. */
  navigator: ReactNode;
  renderPanel(panel: Panel): ReactNode;
  onResize(e: ReactPointerEvent<HTMLDivElement>): void;
  onResetWidth(): void;
}

export default function LeftColumn({ width, navigator, renderPanel, onResize, onResetWidth }: LeftColumnProps) {
  const t = useAgTranslation();
  const historyOpen = useEditorLayout((s) => s.panels.historyOpen);
  const historyHeight = useEditorLayout((s) => s.panels.historyHeightPx) ?? HISTORY_START_PX;
  const changePanels = useEditorLayout((s) => s.changePanels);
  const column = useRef<HTMLDivElement>(null);
  const [cameraOpen, setCameraOpen] = useState(false);

  // Dragging the line up gives History more of the column, down gives Folders more.
  const dragHistory = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const line = e.currentTarget;
    line.setPointerCapture(e.pointerId);
    const startY = e.clientY;
    const max = Math.max(HISTORY_MIN_PX, (column.current?.clientHeight ?? 0) - FOLDERS_MIN_PX);
    const move = (ev: PointerEvent) => {
      const height = Math.round(Math.min(max, Math.max(HISTORY_MIN_PX, historyHeight + startY - ev.clientY)));
      changePanels({ historyHeightPx: height });
    };
    const end = () => {
      line.removeEventListener('pointermove', move);
      line.removeEventListener('pointerup', end);
      line.removeEventListener('pointercancel', end);
    };
    line.addEventListener('pointermove', move);
    line.addEventListener('pointerup', end);
    line.addEventListener('pointercancel', end);
  };
  // Tethering is in today's workspace only where it is supported.
  const tethering = useUIStore((s) => Object.values(s.panelLayout).some((panels) => panels.includes(Panel.Tethering)));

  return (
    <div className="agm-col agm-left" style={{ width }} ref={column}>
      {navigator}
      <div className="agm-folders">{renderPanel(Panel.FolderTree)}</div>
      {tethering && (
        <div className="flex flex-col min-h-0 border-b border-[var(--agm-line)]">
          <div className="agm-sec-head" onClick={() => setCameraOpen((o) => !o)}>
            {cameraOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            {t('mwCamera')}
          </div>
          {cameraOpen && <div className="max-h-[320px] overflow-y-auto">{renderPanel(Panel.Tethering)}</div>}
        </div>
      )}
      {historyOpen && (
        <Resizer
          direction={Orientation.Horizontal}
          onMouseDown={dragHistory}
          onDoubleClick={() => changePanels({ historyHeightPx: null })}
        />
      )}
      <div className="agm-history" style={historyOpen ? { height: historyHeight } : undefined}>
        <div className="agm-sec-head" onClick={() => changePanels({ historyOpen: !historyOpen })}>
          {historyOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          {t('layoutHistory')}
        </div>
        {historyOpen && (
          <div className="agm-history-body">
            <HistoryPanel bare />
          </div>
        )}
      </div>
      <div className="agm-resize" onPointerDown={onResize} onDoubleClick={onResetWidth} />
    </div>
  );
}
