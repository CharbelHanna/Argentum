/**
 * Community presets (their CommunityPage), opened from the Looks tab's menu. Ours.
 *
 * The page takes the window's centre (activeView 'community'), which leaves the
 * editor. Opened over the open photo, it gives the photo back: its back arrow
 * returns there, and so does any change the Looks tab makes to the photo while
 * the page is up, so the change shows.
 *
 * A preset saved on the page goes into My presets > Community
 * (save_community_preset). The Looks tab reads its list once and writes the
 * whole list back on its next change, so it reads it again after each save:
 * otherwise the new preset would not show, and that next change would drop it.
 */

import { create } from 'zustand';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';

interface CommunityState {
  /** The page was opened over the photo open in the editor. */
  overPhoto: boolean;
  /** Presets saved from the page so far; the Looks tab reads its list again when it moves. */
  saved: number;
}

export const useCommunity = create<CommunityState>(() => ({ overPhoto: false, saved: 0 }));

export const openCommunity = (overPhoto: boolean) => {
  useCommunity.setState({ overPhoto });
  useUIStore.getState().setUI({ activeView: 'community' });
};

/** Back where the page was opened: the photo while it is still open, else the grid. */
export const leaveCommunity = () => {
  const toPhoto = useCommunity.getState().overPhoto && !!useEditorStore.getState().selectedImage;
  useUIStore.getState().setUI({ activeView: toPhoto ? 'editor' : 'library' });
};

export const noteCommunitySave = () => useCommunity.setState((s) => ({ saved: s.saved + 1 }));

/** The page is up, over the open photo. */
export const useCommunityOverPhoto = () => {
  const showing = useUIStore((s) => s.activeView === 'community');
  const overPhoto = useCommunity((s) => s.overPhoto);
  return showing && overPhoto;
};
