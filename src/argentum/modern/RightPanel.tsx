/**
 * The Modern window's right panel: every tool, in one panel. Ours.
 *
 * Top to bottom: the histogram (one, for every tab), Info (folded, its header
 * a one-line summary), the tab strip, and the open tab. The tabs are today's
 * panels as they are: Edit is the adjustments, Looks the presets, Crop, Masks,
 * Retouch (today's AI panel) and Export. Their keys are today's (D, P, R, M, K,
 * E), which set the same active panel these tabs read.
 *
 * In the grid nothing is open, so the Edit tab shows the selected photo's
 * settings read-only (gridAdjustments.ts); Crop, Masks and Retouch need the
 * photo itself, so choosing one opens it.
 */

import { type ReactNode, type PointerEvent as ReactPointerEvent, useState } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { ChevronDown, ChevronRight, Info } from 'lucide-react';
import clsx from 'clsx';
import { Panel } from '../../components/ui/AppProperties';
import Controls from '../../components/panel/right/ControlsPanel';
import MasksPanel from '../../components/panel/right/MasksPanel';
import { useEditorStore } from '../../store/useEditorStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useUIStore } from '../../store/useUIStore';
import HistogramSection from '../layout/HistogramSection';
import { useAgTranslation, type AgKey } from '../locales';
import { useGridAdjustments } from './gridAdjustments';

const TABS: { panel: Panel; label: AgKey; key: string; needsPhoto?: boolean }[] = [
  { panel: Panel.Adjustments, label: 'mwTabEdit', key: 'D' },
  { panel: Panel.Presets, label: 'mwTabLooks', key: 'P' },
  { panel: Panel.Crop, label: 'mwTabCrop', key: 'R', needsPhoto: true },
  { panel: Panel.Masks, label: 'mwTabMasks', key: 'M', needsPhoto: true },
  { panel: Panel.Ai, label: 'mwTabRetouch', key: 'K', needsPhoto: true },
  { panel: Panel.Export, label: 'mwTabExport', key: 'E' },
];

/** The graph's height: room for the curve and its clipping marks, not a second photo. */
const HISTOGRAM_PX = 104;

interface RightPanelProps {
  width: number;
  renderPanel(panel: Panel): ReactNode;
  onOpenPhoto(path: string): void;
  onResize(e: ReactPointerEvent<HTMLDivElement>): void;
  onResetWidth(): void;
}

export default function RightPanel({ width, renderPanel, onOpenPhoto, onResize, onResetWidth }: RightPanelProps) {
  const t = useAgTranslation();
  const { activeView, activePanel, setUI } = useUIStore(
    useShallow((s) => ({ activeView: s.activeView, activePanel: s.activePanel, setUI: s.setUI })),
  );
  const selected = useEditorStore((s) => s.selectedImage);
  const libraryActivePath = useLibraryStore((s) => s.libraryActivePath);
  const editing = activeView === 'editor' && !!selected;
  const tab = TABS.find((entry) => entry.panel === activePanel) ?? TABS[0];
  const [infoOpen, setInfoOpen] = useState(false);

  const choose = (entry: (typeof TABS)[number]) => {
    setUI({ activePanel: entry.panel, renderedPanel: entry.panel });
    if (entry.needsPhoto && !editing && libraryActivePath) onOpenPhoto(libraryActivePath);
  };

  const exif = editing ? (selected?.exif as Record<string, string> | undefined) : undefined;
  const summary = [exif?.Model, exif?.LensModel].filter(Boolean).join(' · ');

  return (
    <div className="agm-col agm-right" style={{ width }}>
      <div className="agm-resize" onPointerDown={onResize} onDoubleClick={onResetWidth} />
      <div className="flex-none">
        <HistogramSection height={HISTOGRAM_PX} />
      </div>
      <div className="flex-none border-b border-[var(--agm-line)]">
        <div className="agm-sec-head" onClick={() => setInfoOpen((o) => !o)} data-tooltip={t('mwInfoTip')}>
          {infoOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          {t('mwInfo')}
          {!infoOpen && summary && <span className="agm-info-line">{summary}</span>}
        </div>
        {infoOpen && <div className="max-h-[320px] overflow-y-auto">{renderPanel(Panel.Metadata)}</div>}
      </div>
      <div className="agm-tabs" role="tablist">
        {TABS.map((entry) => (
          <button
            key={entry.panel}
            role="tab"
            aria-selected={entry === tab}
            className={clsx('agm-tab', entry === tab && 'agm-on', entry.needsPhoto && !editing && 'agm-dimtab')}
            onClick={() => choose(entry)}
            data-tooltip={`${t(entry.label)} (${entry.key})`}
          >
            {t(entry.label)}
          </button>
        ))}
      </div>
      <div className="agm-tab-body">
        <div className="agm-tab-scroll custom-scrollbar">
          <TabBody tab={tab.panel} editing={editing} renderPanel={renderPanel} />
        </div>
      </div>
    </div>
  );
}

function TabBody({
  tab,
  editing,
  renderPanel,
}: {
  tab: Panel;
  editing: boolean;
  renderPanel(panel: Panel): ReactNode;
}) {
  const t = useAgTranslation();
  const libraryActivePath = useLibraryStore((s) => s.libraryActivePath);
  const selectedCount = useLibraryStore((s) => s.multiSelectedPaths.length);
  const gridAdjustments = useGridAdjustments(editing ? null : libraryActivePath);
  const name = libraryActivePath?.split(/[\\/]/).pop();

  if (tab === Panel.Adjustments) {
    if (editing) return <Controls showWaveform={false} />;
    if (!libraryActivePath) return <Quiet text={t('mwNoPhoto')} />;
    return (
      <>
        <div className="agm-readonly-strip">
          <Info size={13} className="shrink-0 mt-px" />
          <span>
            {t('mwReadOnlyShowing')} <b>{name}</b> {t('mwReadOnlyHow')}
            {selectedCount > 1 && <> {t('mwReadOnlyMany').replace('{count}', String(selectedCount))}</>}
          </span>
        </div>
        {gridAdjustments && <Controls showWaveform={false} readOnly={{ adjustments: gridAdjustments }} />}
      </>
    );
  }
  if (tab === Panel.Masks) return editing ? <MasksPanel showWaveform={false} /> : <Quiet text={t('mwOpenToUse')} />;
  if ((tab === Panel.Crop || tab === Panel.Ai) && !editing) return <Quiet text={t('mwOpenToUse')} />;
  return <>{renderPanel(tab)}</>;
}

function Quiet({ text }: { text: string }) {
  return <div className="agm-hint pt-4">{text}</div>;
}
