/**
 * The Modern window's centre before a folder is picked: a first run, or a
 * folder whose disk is unplugged. Ours. Not a start screen: the whole window is
 * there around it, and the folder column on the left is the way in.
 */

import { useAgTranslation } from '../locales';
import { useModernFolders } from './folders';

export default function ModernEmpty() {
  const t = useAgTranslation();
  const unavailable = useModernFolders((s) => s.unavailable);
  return (
    <div className="agm-empty">
      <div className="agm-empty-arrow">←</div>
      <div className="agm-empty-title">{t('mwEmptyTitle')}</div>
      <div className="agm-empty-note">{unavailable ? t('mwNoDisks') : t('mwEmptyNote')}</div>
      <div className="agm-empty-note">
        {t('mwEmptyPaste')} <kbd>Alt+D</kbd>
      </div>
    </div>
  );
}
