/**
 * Where each of today's panels shows in the Modern window. Ours.
 *
 * The right panel's tab strip, in the order a photo is worked on, and the
 * Info line's tab, read by RightPanel.tsx; the folder tree and the camera
 * are the left column's (LeftColumn.tsx). A panel's key or menu item opens
 * the column named here (useUIStore `setPanel`), so this list is the one
 * place that says where a panel lives.
 */

import { Panel } from '../../components/ui/AppProperties';
import type { AgKey } from '../locales';

export interface RightTab {
  panel: Panel;
  label: AgKey;
  key: string;
  needsPhoto?: boolean;
}

export const RIGHT_TABS: readonly RightTab[] = [
  { panel: Panel.Adjustments, label: 'mwTabEdit', key: 'D' },
  { panel: Panel.Crop, label: 'mwTabCrop', key: 'R', needsPhoto: true },
  { panel: Panel.Presets, label: 'mwTabLooks', key: 'P' },
  { panel: Panel.Masks, label: 'mwTabMasks', key: 'M', needsPhoto: true },
  { panel: Panel.Ai, label: 'mwTabRetouch', key: 'K', needsPhoto: true },
  { panel: Panel.Export, label: 'mwTabExport', key: 'E' },
];

/** The Info line's tab: today's metadata panel, shown at the right panel's full height. */
export const INFO_TAB: RightTab = { panel: Panel.Metadata, label: 'mwInfo', key: 'I' };

const LEFT_PANELS: readonly Panel[] = [Panel.FolderTree, Panel.Tethering];

/** The column `panel` shows in. */
export const columnOf = (panel: Panel): 'left' | 'right' => (LEFT_PANELS.includes(panel) ? 'left' : 'right');
