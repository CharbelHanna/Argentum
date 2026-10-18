/**
 * Settings > General > Theme in the Modern window: Grey (the default), Deep or
 * Light. Ours. Classic keeps the app's own theme list; this choice is stored
 * beside the layout (mods/editor_layout.rs) and only the Modern window reads it.
 */

import Dropdown from '../../components/ui/Dropdown';
import { useAgTranslation } from '../locales';
import { type ModernThemeName, useEditorLayout } from '../layout/editorLayout';

export default function ModernThemeChoice() {
  const t = useAgTranslation();
  const theme = useEditorLayout((s) => s.theme);
  const chooseTheme = useEditorLayout((s) => s.chooseTheme);
  return (
    <Dropdown<ModernThemeName>
      onChange={(value) => chooseTheme(value)}
      options={[
        { value: 'grey', label: t('mwThemeGrey') },
        { value: 'deep', label: t('mwThemeDeep') },
        { value: 'light', label: t('mwThemeLight') },
      ]}
      value={theme}
      triggerClassName="bg-bg-primary"
    />
  );
}
