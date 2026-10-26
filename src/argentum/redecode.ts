/**
 * Decode the open photo again, for the settings that act while a RAW is
 * decoded: highlight recovery and the RAW decoder. Ours.
 *
 * `redecode_open_photo` decodes it to one side and swaps it in (see
 * `mods/redecode.rs` for why it is not `load_image`); then one render of the
 * same edits, through the same path a slider uses.
 */

import { useEditorStore } from '../store/useEditorStore';
import { ag } from './ag';

export async function redecodeOpenPhoto(): Promise<void> {
  if (await ag<boolean>('redecode_open_photo')) {
    // A new adjustments object with the same values: the render effect fires
    // on identity, exactly as after a slider, and nothing changes.
    useEditorStore.getState().setEditor((s) => ({ adjustments: { ...s.adjustments } }));
  }
}
