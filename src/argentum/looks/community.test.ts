import { beforeEach, describe, expect, it } from 'vitest';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';
import type { SelectedImage } from '../../components/ui/AppProperties';
import { leaveCommunity, noteCommunitySave, openCommunity, useCommunity } from './community';

const view = () => useUIStore.getState().activeView;

describe('Community presets', () => {
  beforeEach(() => {
    useCommunity.setState({ overPhoto: false, saved: 0 });
    useEditorStore.setState({ selectedImage: { path: 'a.raf' } as SelectedImage });
  });

  it('opened over the photo, goes back to the photo', () => {
    useUIStore.getState().setUI({ activeView: 'editor' });
    openCommunity(true);
    expect(view()).toBe('community');
    leaveCommunity();
    expect(view()).toBe('editor');
  });

  it('opened from the grid, goes back to the grid', () => {
    useUIStore.getState().setUI({ activeView: 'library' });
    openCommunity(false);
    leaveCommunity();
    expect(view()).toBe('library');
  });

  it('goes back to the grid when the photo it was opened over has closed', () => {
    openCommunity(true);
    useEditorStore.setState({ selectedImage: null });
    leaveCommunity();
    expect(view()).toBe('library');
  });

  it('counts the presets saved, for the Looks tab to read its list again', () => {
    noteCommunitySave();
    noteCommunitySave();
    expect(useCommunity.getState().saved).toBe(2);
  });
});
