/**
 * The grid's view when a filter left it empty: what is hiding the photos, and
 * the way back to all of them. Ours, after Shoebox's banner (src/App.tsx, "No
 * photos here match these filters", with its "Clear filters" button).
 */

import type { MouseEvent } from 'react';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useAgTranslation } from '../locales';
import { shownFiltersOnly } from './shownFilters';

export default function NoPhotosMatch({
  filters,
  onContextMenu,
}: {
  filters: string;
  onContextMenu(event: MouseEvent): void;
}) {
  const t = useAgTranslation();
  const setFilterCriteria = useLibraryStore((s) => s.setFilterCriteria);
  return (
    <div className="agm-empty" onContextMenu={onContextMenu}>
      <div className="agm-empty-title">{t('mwNoMatch').replace('{filters}', filters)}</div>
      <button className="agm-btn agm-btn-box" onClick={() => setFilterCriteria(shownFiltersOnly({}))}>
        {t('mwClearFilters')}
      </button>
    </div>
  );
}
