/**
 * The Modern window's left column: where the photos are (Folders) and when the
 * edits happened (History), plus the camera while tethered shooting is
 * available. Ours.
 *
 * Folders is today's folder tree in its Modern form (FolderTree.tsx: Favourites,
 * Recent, This computer). History is today's undo steps (layout/HistoryPanel).
 * The column is resizable from its right edge; double-click resets it.
 */

import { type ReactNode, type PointerEvent as ReactPointerEvent, useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Panel } from '../../components/ui/AppProperties';
import { useUIStore } from '../../store/useUIStore';
import HistoryPanel from '../layout/HistoryPanel';
import { useAgTranslation } from '../locales';

interface LeftColumnProps {
  width: number;
  renderPanel(panel: Panel): ReactNode;
  onResize(e: ReactPointerEvent<HTMLDivElement>): void;
  onResetWidth(): void;
}

export default function LeftColumn({ width, renderPanel, onResize, onResetWidth }: LeftColumnProps) {
  const t = useAgTranslation();
  const [historyOpen, setHistoryOpen] = useState(true);
  const [cameraOpen, setCameraOpen] = useState(false);
  // Tethering is in today's workspace only where it is supported.
  const tethering = useUIStore((s) => Object.values(s.panelLayout).some((panels) => panels.includes(Panel.Tethering)));

  return (
    <div className="agm-col agm-left" style={{ width }}>
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
      <div className="agm-history">
        <div className="agm-sec-head" onClick={() => setHistoryOpen((o) => !o)}>
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
