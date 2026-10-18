/**
 * The Recent group in the Modern folder column. Ours.
 *
 * The folders opened most recently, newest first (mods/recent_folders.rs),
 * drawn as the tree's own rows so the column reads as one list. Pointing at a
 * row shows the folder's full path; clicking opens it as if picked in the tree,
 * which unfolds the tree down to it.
 */

import clsx from 'clsx';
import { Clock } from 'lucide-react';
import Text from '../../components/ui/Text';
import { TextColors, TextWeights } from '../../types/typography';
import { folderName, useModernFolders } from './folders';

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
        <Text key={path} as="div" color={TextColors.primary} weight={TextWeights.medium}>
          <div
            className={clsx('flex items-center gap-2 p-1.5 rounded-md cursor-pointer', {
              'bg-surface': path === selectedPath,
            })}
            data-tooltip={path}
            onClick={() => onFolderSelect(path)}
          >
            <div className="w-5 h-5 flex items-center justify-center p-0.5 text-text-secondary shrink-0">
              <Clock size={14} />
            </div>
            <span className="min-w-0 flex-1 truncate select-none">{folderName(path)}</span>
          </div>
        </Text>
      ))}
    </div>
  );
}
