// Settings > Reset Layout keeps the left column's Camera section where tethering is supported:
// the store knows that from start-up, so no caller can say otherwise.

import { describe, expect, it } from 'vitest';
import { Panel } from '../components/ui/AppProperties';
import { useUIStore } from './useUIStore';

const panelsAfterReset = () => Object.values(useUIStore.getState().resetWorkspaceLayout().panelLayout).flat();

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
