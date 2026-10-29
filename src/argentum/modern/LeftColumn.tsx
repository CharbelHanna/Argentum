/**
 * The Modern window's left column: the Navigator while a photo is open, where
 * the photos are (Folders) and when the edits happened (Snapshots, History),
 * plus the camera while tethered shooting is available. Ours.
 *
 * Folders is today's folder tree in its Modern form (FolderTree.tsx: Favourites,
 * Recent, This computer). Snapshots and History (today's undo steps,
 * layout/HistoryPanel) each have a line above them that drags to give them
 * more or less of the column; a double-click on one puts it back. The column is resizable from its right edge; double-click
 * resets it. What is folded and History's height are saved with the layout.
 */

import { type ReactNode, type PointerEvent as ReactPointerEvent, useRef, useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Orientation, Panel } from '../../components/ui/AppProperties';
import Resizer from '../../components/ui/Resizer';
import { useUIStore } from '../../store/useUIStore';
import HistoryPanel from '../layout/HistoryPanel';
import SnapshotsSection, { SNAPSHOTS_PANE } from '../snapshots/SnapshotsSection';
import { useEditorLayout } from '../layout/editorLayout';
import { useAgTranslation } from '../locales';

/** History's and the Snapshots list's heights until dragged, and the least each part keeps. */
const HISTORY_START_PX = 220;
const HISTORY_MIN_PX = 72;
const SNAPSHOTS_START_PX = 120;
const SNAPSHOTS_MIN_PX = 48;
const FOLDERS_MIN_PX = 140;

interface LeftColumnProps {
  width: number;
  /** A photo is open: Snapshots can be made and restored. */
  editing: boolean;
  /** The docked Navigator, while a photo is open and it is not floating. */
  navigator: ReactNode;
  renderPanel(panel: Panel): ReactNode;
  onResize(e: ReactPointerEvent<HTMLDivElement>): void;
  onResetWidth(): void;
}

export default function LeftColumn({
  width,
  editing,
  navigator,
  renderPanel,
  onResize,
  onResetWidth,
}: LeftColumnProps) {
  const t = useAgTranslation();
  const historyOpen = useEditorLayout((s) => s.panels.historyOpen);
  const historyHeight = useEditorLayout((s) => s.panels.historyHeightPx) ?? HISTORY_START_PX;
  const snapshotsOpen = useEditorLayout((s) => !s.panels.folded.includes(SNAPSHOTS_PANE));
  const snapshotsHeight = useEditorLayout((s) => s.panels.snapshotsHeightPx) ?? SNAPSHOTS_START_PX;
  const changePanels = useEditorLayout((s) => s.changePanels);
  const folders = useRef<HTMLDivElement>(null);
  const [cameraOpen, setCameraOpen] = useState(false);

  /**
   * Dragging a line up gives the part under it more of the column, taken from
   * Folders, which always keeps its least; down gives Folders more.
   */
  const drag = (e: ReactPointerEvent<HTMLDivElement>, from: number, min: number, keep: (px: number) => void) => {
    e.preventDefault();
    const line = e.currentTarget;
    line.setPointerCapture(e.pointerId);
    const startY = e.clientY;
    const max = Math.max(min, from + (folders.current?.clientHeight ?? 0) - FOLDERS_MIN_PX);
    const move = (ev: PointerEvent) => keep(Math.round(Math.min(max, Math.max(min, from + startY - ev.clientY))));
    const end = () => {
      line.removeEventListener('pointermove', move);
      line.removeEventListener('pointerup', end);
      line.removeEventListener('pointercancel', end);
    };
    line.addEventListener('pointermove', move);
    line.addEventListener('pointerup', end);
    line.addEventListener('pointercancel', end);
  };
  const tethering = useUIStore((s) => s.isTetheringSupported);

  return (
    <div className="agm-col agm-left" style={{ width }}>
      {navigator}
      <div className="agm-folders" ref={folders}>
        {renderPanel(Panel.FolderTree)}
      </div>
      {tethering && (
        <div className="flex flex-col min-h-0 border-b border-[var(--agm-line)]">
          <div className="agm-sec-head" onClick={() => setCameraOpen((o) => !o)}>
            {cameraOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            {t('mwCamera')}
          </div>
          {cameraOpen && <div className="max-h-[320px] overflow-y-auto">{renderPanel(Panel.Tethering)}</div>}
        </div>
      )}
      {snapshotsOpen && (
        <Resizer
          direction={Orientation.Horizontal}
          onMouseDown={(e) =>
            drag(e, snapshotsHeight, SNAPSHOTS_MIN_PX, (px) => changePanels({ snapshotsHeightPx: px }))
          }
          onDoubleClick={() => changePanels({ snapshotsHeightPx: null })}
        />
      )}
      <SnapshotsSection editing={editing} height={snapshotsHeight} />
      {historyOpen && (
        <Resizer
          direction={Orientation.Horizontal}
          onMouseDown={(e) => drag(e, historyHeight, HISTORY_MIN_PX, (px) => changePanels({ historyHeightPx: px }))}
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
