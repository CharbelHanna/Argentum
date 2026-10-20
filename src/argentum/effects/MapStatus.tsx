/**
 * Where Relight and Fog ask for their model, and say they are working. Ours.
 *
 * Under the switch, in the panel, because the switch is right there. The ask
 * has the Subject and Sky masks' words and buttons (selection.tsx); the
 * working line is RapidRAW's own (Effects.tsx: a spinner, and while their
 * download runs, its name from `aiModelDownloadStatus`). Their download sends
 * no progress, only a start and a finish, so the bar runs without a number.
 */

import { Loader2 } from 'lucide-react';
import Text from '../../components/ui/Text';
import { TextVariants } from '../../types/typography';
import { useProcessStore } from '../../store/useProcessStore';
import { formatSize } from '../formatSize';
import { useAgTranslation, type AgKey } from '../locales';
import type { MapPhase } from './effectMap';

interface MapStatusProps {
  phase: MapPhase;
  /** "... needs its ... model ({size}, downloaded once)." */
  ask: AgKey;
  downloading: AgKey;
  working: string;
  onDownload(): void;
  onNotNow(): void;
}

export default function MapStatus({ phase, ask, downloading, working, onDownload, onNotNow }: MapStatusProps) {
  const t = useAgTranslation();
  const isDownloading = useProcessStore((s) => !!s.aiModelDownloadStatus);

  if (phase.kind === 'asking') {
    return (
      <div className="mt-3 p-2 rounded-md bg-bg-primary flex flex-col gap-2">
        <Text variant={TextVariants.small}>{t(ask).replace('{size}', formatSize(phase.bytes))}</Text>
        <div className="flex gap-2">
          <button
            className="px-3 py-1 rounded-md bg-accent text-button-text text-sm font-medium hover:opacity-90"
            onClick={onDownload}
          >
            {t('selDownload')}
          </button>
          <button
            className="px-3 py-1 rounded-md bg-surface text-text-primary text-sm hover:bg-card-active"
            onClick={onNotNow}
          >
            {t('selNotNow')}
          </button>
        </div>
      </div>
    );
  }

  if (phase.kind === 'working') {
    return (
      <div className="mt-3 p-2 flex flex-col gap-2 text-text-secondary">
        <div className="flex items-center gap-2">
          <Loader2 size={16} className="animate-spin shrink-0" />
          <Text variant={TextVariants.label}>{isDownloading ? t(downloading) : working}</Text>
        </div>
        {isDownloading && (
          <div className="h-1 rounded-full bg-bg-primary overflow-hidden">
            <div className="h-full w-full bg-accent animate-pulse" />
          </div>
        )}
      </div>
    );
  }

  return null;
}
