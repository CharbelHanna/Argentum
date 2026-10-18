/**
 * The Recent group in the Modern folder column. Ours.
 *
 * The folders opened most recently, newest first (mods/recent_folders.rs),
 * drawn as the tree's own rows so the column reads as one list, with their
 * photo counts and the tree's keys. Pointing at a row shows the folder's full
 * path; clicking opens it as if picked in the tree, which unfolds the tree
 * down to it.
 */

import clsx from 'clsx';
import { Clock } from 'lucide-react';
import Text from '../../components/ui/Text';
import { TextColors, TextVariants, TextWeights } from '../../types/typography';
import { useFolderCount } from './folderCounts';
import { folderName, useModernFolders } from './folders';
import { treeRow } from './treeKeys';

/** The tree's open-sections key that folds Recent. Open unless folded. */
export const RECENT_FOLDED = 'recent-folded';

export default function RecentFolders({
  onFolderSelect,
  selectedPath,
}: {
  onFolderSelect(path: string): void;
  selectedPath: string | null;
}) {
  const recent = useModernFolders((s) => s.recent);

  return (
    <div className="pb-1">
      {recent.map((path) => (
        <RecentRow key={path} path={path} selected={path === selectedPath} onSelect={() => onFolderSelect(path)} />
      ))}
    </div>
  );
}

function RecentRow({ path, selected, onSelect }: { path: string; selected: boolean; onSelect(): void }) {
  const count = useFolderCount(path, true);
  return (
    <Text as="div" color={TextColors.primary} weight={TextWeights.medium}>
      <div
        className={clsx('flex items-center gap-2 p-1.5 rounded-md cursor-pointer', { 'bg-surface': selected })}
        data-tooltip={path}
        onClick={onSelect}
        {...treeRow({ depth: 0, opens: false, open: false, selected })}
      >
        <div className="w-5 h-5 flex items-center justify-center p-0.5 text-text-secondary shrink-0">
          <Clock size={14} />
        </div>
        <span className="min-w-0 flex-1 truncate select-none">{folderName(path)}</span>
        {!!count && (
          <Text as="span" variant={TextVariants.small} color={TextColors.secondary} className="shrink-0 tabular-nums">
            {count.toLocaleString()}
          </Text>
        )}
        <div className="w-5 h-5 shrink-0" aria-hidden="true" />
      </div>
    </Text>
  );
}
