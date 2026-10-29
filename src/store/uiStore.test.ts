// Settings > Reset Layout keeps the left column's Camera section where tethering is supported:
// the store knows that from start-up, so no caller can say otherwise.

import { describe, expect, it } from 'vitest';
import { Panel } from '../components/ui/AppProperties';
import { useUIStore } from './useUIStore';

const panelsAfterReset = () => Object.values(useUIStore.getState().resetWorkspaceLayout().panelLayout).flat();

// A panel's key or menu item opens the column the Modern window shows it in (argentum/modern/panels.ts).
describe('setPanel', () => {
  const hidden = { filmstrip: true, leftPanel: false, rightPanel: false };

  it('opens the right column on a right panel tab, Export and Info included', () => {
    for (const panel of [Panel.Export, Panel.Metadata, Panel.Crop]) {
      useUIStore.setState({ uiVisibility: hidden, activePanel: Panel.Adjustments });
      useUIStore.getState().setPanel(panel);
      const { uiVisibility, activePanel } = useUIStore.getState();
      expect([panel, uiVisibility.rightPanel, uiVisibility.leftPanel]).toEqual([panel, true, false]);
      expect(activePanel).toBe(panel);
    }
  });

  it('opens the left column for the folders, leaving the right panel tab as it was', () => {
    useUIStore.setState({ uiVisibility: hidden, activePanel: Panel.Crop });
    useUIStore.getState().setPanel(Panel.FolderTree);
    const { uiVisibility, activePanel } = useUIStore.getState();
    expect([uiVisibility.leftPanel, uiVisibility.rightPanel, activePanel]).toEqual([true, false, Panel.Crop]);
  });
});

describe('Reset Layout', () => {
  it('keeps the Camera section where tethering is supported', () => {
    useUIStore.setState({ isTetheringSupported: true });
    expect(panelsAfterReset()).toContain(Panel.Tethering);
  });

  it('leaves it out where tethering is not supported', () => {
    useUIStore.setState({ isTetheringSupported: false });
    expect(panelsAfterReset()).not.toContain(Panel.Tethering);
  });
});
