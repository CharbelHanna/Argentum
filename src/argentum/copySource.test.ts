// Which photo Copy takes settings from (copySourcePath in hooks/useEditorActions.ts).
// RapidRAW d14e19da.
import { describe, expect, it } from 'vitest';
import { copySourcePath } from '../hooks/useEditorActions';

describe('the photo Copy takes settings from', () => {
  it('is the grid’s active photo, not the photo last loaded, while the grid shows', () => {
    // a.cr3 was clicked, then b.cr3 ctrl-clicked and a.cr3 ctrl-clicked out again.
    const from = { editorOpen: false, loadedPath: 'a.cr3', libraryActivePath: 'b.cr3', multiSelectedPaths: ['b.cr3'] };
    expect(copySourcePath(from)).toBe('b.cr3');
  });

  it('is the photo open in the editor while it is open', () => {
    const from = { editorOpen: true, loadedPath: 'a.cr3', libraryActivePath: 'b.cr3', multiSelectedPaths: ['b.cr3'] };
    expect(copySourcePath(from)).toBe('a.cr3');
  });

  it('is the photo named, wherever it is', () => {
    const from = { editorOpen: true, loadedPath: 'a.cr3', libraryActivePath: 'b.cr3', multiSelectedPaths: [] };
    expect(copySourcePath({ ...from, named: 'c.cr3' })).toBe('c.cr3');
  });

  it('falls back to the first selected, then to the photo last loaded', () => {
    const none = { editorOpen: false, libraryActivePath: null, multiSelectedPaths: [] };
    expect(copySourcePath({ ...none, multiSelectedPaths: ['d.cr3', 'e.cr3'], loadedPath: 'a.cr3' })).toBe('d.cr3');
    expect(copySourcePath({ ...none, loadedPath: 'a.cr3' })).toBe('a.cr3');
    expect(copySourcePath(none)).toBeUndefined();
  });
});
