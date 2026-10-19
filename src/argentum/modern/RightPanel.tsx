/**
 * The Modern window's right panel: every tool, in one panel. Ours.
 *
 * Top to bottom: the histogram (one, for every tab), the Info line, the tab
 * strip, and the open tab. The tabs are today's panels as they are, in the
 * order a photo is worked on: Edit is the adjustments, then Crop, Looks (ours,
 * argentum/looks: presets and LUTs, the film looks among the LUTs), Masks,
 * Retouch (today's AI panel) and Export. The Info line, the photo's ISO, aperture, shutter and focal length,
 * is a tab of its own without a seventh name in the strip: a click on it, or
 * I, shows today's metadata panel at the panel's full height, and any tab goes
 * back. The keys are today's (D, P, R, M, K, E, I), which set the same active
 * panel these tabs read.
 *
 * In the grid nothing is open, so the Edit tab shows the selected photo's
 * settings read-only (gridAdjustments.ts); Crop, Masks and Retouch need the
 * photo itself, so choosing one opens it.
 */

import { type ReactNode, type PointerEvent as ReactPointerEvent } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { Info } from 'lucide-react';
import clsx from 'clsx';
import { Panel } from '../../components/ui/AppProperties';
import Controls from '../../components/panel/right/ControlsPanel';
import MasksPanel from '../../components/panel/right/MasksPanel';
import MetadataPanel from '../../components/panel/right/MetadataPanel';
import { useEditorStore } from '../../store/useEditorStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useUIStore } from '../../store/useUIStore';
import HistogramSection from '../layout/HistogramSection';
import { useAgTranslation, type AgKey } from '../locales';
import LooksPanel from '../looks/LooksPanel';
import { useGridAdjustments } from './gridAdjustments';

const TABS: { panel: Panel; label: AgKey; key: string; needsPhoto?: boolean }[] = [
  { panel: Panel.Adjustments, label: 'mwTabEdit', key: 'D' },
  { panel: Panel.Crop, label: 'mwTabCrop', key: 'R', needsPhoto: true },
  { panel: Panel.Presets, label: 'mwTabLooks', key: 'P' },
  { panel: Panel.Masks, label: 'mwTabMasks', key: 'M', needsPhoto: true },
  { panel: Panel.Ai, label: 'mwTabRetouch', key: 'K', needsPhoto: true },
  { panel: Panel.Export, label: 'mwTabExport', key: 'E' },
];

/** The Info line's tab: today's metadata panel. */
const INFO = { panel: Panel.Metadata, label: 'mwInfo' as AgKey, key: 'I' };

/**
 * The Info line: ISO, aperture, shutter and focal length, each written the way
 * their editor toolbar and metadata panel write it (EditorToolbar.tsx and
 * MetadataPanel.tsx, KEY_CAMERA_SETTINGS_MAP, RapidRAW 1.6.5 @ 79c2a46b).
 */
function infoLine(exif: Record<string, string> | null | undefined): string {
  if (!exif) return '';
  const iso = exif.PhotographicSensitivity || exif.ISO;
  const aperture = exif.FNumber && (/^f/i.test(exif.FNumber) ? exif.FNumber : `f/${exif.FNumber}`);
  const shutter = exif.ExposureTime && (exif.ExposureTime.endsWith('s') ? exif.ExposureTime : `${exif.ExposureTime}s`);
  const focal = exif.FocalLengthIn35mmFilm || exif.FocalLength;
  return [iso && `ISO ${iso}`, aperture, shutter, focal && (String(focal).endsWith('mm') ? focal : `${focal} mm`)]
    .filter(Boolean)
    .join(' · ');
}

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
  // In the grid, the selected photo's, as the library read it.
  const libraryExif = useLibraryStore((s) => s.imageList.find((image) => image.path === s.libraryActivePath)?.exif);
  const editing = activeView === 'editor' && !!selected;
  const tab = activePanel === INFO.panel ? INFO : (TABS.find((entry) => entry.panel === activePanel) ?? TABS[0]);

  const choose = (entry: (typeof TABS)[number] | typeof INFO) => {
    setUI({ activePanel: entry.panel, renderedPanel: entry.panel });
    if ('needsPhoto' in entry && entry.needsPhoto && !editing && libraryActivePath) onOpenPhoto(libraryActivePath);
  };

  const summary = infoLine(editing ? (selected?.exif as Record<string, string> | undefined) : libraryExif);

  return (
    <div className="agm-col agm-right" style={{ width }}>
      <div className="agm-resize" onPointerDown={onResize} onDoubleClick={onResetWidth} />
      <div className="flex-none">
        <HistogramSection height={HISTOGRAM_PX} />
      </div>
      <div className="flex-none">
        <div
          className={clsx('agm-sec-head agm-info-head', tab === INFO && 'agm-on')}
          onClick={() => choose(INFO)}
          data-tooltip={t('mwInfoTip')}
        >
          {t('mwInfo')}
          {summary && <span className="agm-info-line">{summary}</span>}
        </div>
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
  const libraryExif = useLibraryStore((s) => s.imageList.find((image) => image.path === s.libraryActivePath)?.exif);
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
  if (tab === Panel.Presets) return <LooksPanel editing={editing} />;
  if (tab === Panel.Masks) return editing ? <MasksPanel showWaveform={false} /> : <Quiet text={t('mwOpenToUse')} />;
  if (tab === Panel.Metadata && !editing) {
    // The grid's selection, from what the library read of it.
    return libraryActivePath ? (
      <MetadataPanel photo={{ path: libraryActivePath, exif: libraryExif ?? null }} />
    ) : (
      <Quiet text={t('mwNoPhoto')} />
    );
  }
  if ((tab === Panel.Crop || tab === Panel.Ai) && !editing) return <Quiet text={t('mwOpenToUse')} />;
  return <>{renderPanel(tab)}</>;
}

function Quiet({ text }: { text: string }) {
  return <div className="agm-hint pt-4">{text}</div>;
}
