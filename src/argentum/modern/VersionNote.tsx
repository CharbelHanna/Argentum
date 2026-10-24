/**
 * The version beside the Settings gear, quiet; "Argentum 26.43.5 is out" in the
 * accent colour when a newer release is published, opening its release page.
 * Ours, the way Shoebox shows it beside its own Settings (Sidebar.tsx).
 */

import { open } from '@tauri-apps/plugin-shell';
import { useAgTranslation } from '../locales';
import { useUpdateStatus } from '../updates';

export default function VersionNote() {
  const t = useAgTranslation();
  const update = useUpdateStatus();
  if (!update.current) return null;

  if (update.newer && update.latest) {
    return (
      <button
        className="agm-btn agm-version agm-version-new"
        onClick={() => void open(update.url)}
        data-tooltip={t('updateTip')}
      >
        {t('updateOut').replace('{version}', update.latest)}
      </button>
    );
  }
  return (
    <span className="agm-version" data-tooltip={t('versionTip').replace('{version}', update.current)}>
      {t('versionShort').replace('{version}', update.current)}
    </span>
  );
}
