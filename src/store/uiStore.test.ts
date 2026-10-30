// Settings > Reset Layout and what start-up reads back: the two column widths are all the window keeps.

import { describe, expect, it } from 'vitest';
import { Panel } from '../components/ui/AppProperties';
import { DEFAULT_PANEL_WIDTH, readWorkspace, useUIStore } from './useUIStore';

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
  it('gives back only the two default widths', () => {
    useUIStore.setState({ leftPanelWidth: 500, rightPanelWidth: 600 });
    const defaults = { leftPanelWidth: DEFAULT_PANEL_WIDTH, rightPanelWidth: DEFAULT_PANEL_WIDTH };
    expect(useUIStore.getState().resetWorkspaceLayout()).toEqual(defaults);
    expect(useUIStore.getState().leftPanelWidth).toBe(DEFAULT_PANEL_WIDTH);
    expect(useUIStore.getState().activePanel).toBe(Panel.Adjustments);
  });

  it('leaves the Camera section as the machine allows', () => {
    useUIStore.setState({ isTetheringSupported: true });
    useUIStore.getState().resetWorkspaceLayout();
    expect(useUIStore.getState().isTetheringSupported).toBe(true);
  });
});

describe('readWorkspace', () => {
  it('keeps saved widths from a file written with only the widths', () => {
    expect(readWorkspace({ leftPanelWidth: 280, rightPanelWidth: 360 })).toEqual({
      leftPanelWidth: 280,
      rightPanelWidth: 360,
    });
  });

  it('drops what the Classic layout saved', () => {
    const saved = { leftPanelWidth: 280, rightPanelWidth: 360, leftTopHeight: 400, panelLayout: { leftTop: [] } };
    expect(readWorkspace(saved)).toEqual({ leftPanelWidth: 280, rightPanelWidth: 360 });
  });

  it('uses the default width for a missing workspace', () => {
    expect(readWorkspace(undefined)).toEqual({
      leftPanelWidth: DEFAULT_PANEL_WIDTH,
      rightPanelWidth: DEFAULT_PANEL_WIDTH,
    });
  });
});
