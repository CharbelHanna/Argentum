// A rating or flag that hides the active photo under the library's filter offers Undo, which puts the
// old value back in the store and on disk. hooks/useLibraryActions.ts and argentum/filterExitUndo.ts,
// RapidRAW 4eb6b2da and 18ebf030.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const backend = vi.hoisted(() => ({ calls: [] as [string, unknown][] }));
const popup = vi.hoisted(() => ({ shown: [] as { props: { fileName: string; count: number; onUndo: () => void } }[] }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (command: string, args?: unknown) => {
    backend.calls.push([command, args]);
    return Promise.resolve();
  },
}));

vi.mock('react-toastify', () => ({
  toast: Object.assign((content: (typeof popup.shown)[number]) => popup.shown.push(content), {
    dismiss: () => {},
    error: () => {},
    isActive: () => false,
    update: () => {},
  }),
}));

// The hook uses no React hook but useCallback, so it can be called outside a component.
vi.mock('react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('react')>()),
  useCallback: <T>(fn: T) => fn,
}));

import { useLibraryActions } from '../hooks/useLibraryActions';
import { computeSortedLibrary } from '../hooks/useSortedLibrary';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';
import { FlagStatus, ImageFlag, Invokes, RawStatus, type ImageFile } from '../components/ui/AppProperties';

const photo = (path: string, flag: ImageFlag | null = null) => ({ path, flag, tags: null }) as unknown as ImageFile;
const shown = () => computeSortedLibrary(useLibraryStore.getState(), useSettingsStore.getState()).map((i) => i.path);
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));
const actions = () => useLibraryActions();

const showOnly = (filter: { rating?: number; flagStatus?: FlagStatus }) =>
  useLibraryStore.setState({ filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All, ...filter } });

describe('undo when a photo leaves the filter', () => {
  beforeEach(() => {
    backend.calls.length = 0;
    popup.shown.length = 0;
    useUIStore.setState({ activeView: 'library', imageSelectHandler: null });
    useEditorStore.setState({ selectedImage: null });
    useLibraryStore.setState({
      imageList: [photo('/p/a.CR2'), photo('/p/b.CR2'), photo('/p/c.CR2')],
      imageRatings: { '/p/a.CR2': 3, '/p/b.CR2': 3, '/p/c.CR2': 3 },
      libraryActivePath: '/p/b.CR2',
      multiSelectedPaths: ['/p/b.CR2'],
      selectionAnchorPath: '/p/b.CR2',
    });
    showOnly({});
  });

  it('offers Undo when a rating hides the photo, and Undo brings it back', async () => {
    showOnly({ rating: 3 });

    actions().handleRate(1);
    expect(shown()).toEqual(['/p/a.CR2', '/p/c.CR2']);
    expect(popup.shown).toHaveLength(1);
    expect(popup.shown[0].props).toMatchObject({ fileName: 'b.CR2', count: 1 });

    await settled();
    backend.calls.length = 0;
    popup.shown[0].props.onUndo();
    await settled();

    expect(useLibraryStore.getState().imageRatings['/p/b.CR2']).toBe(3);
    expect(shown()).toEqual(['/p/a.CR2', '/p/b.CR2', '/p/c.CR2']);
    expect(useLibraryStore.getState().libraryActivePath).toBe('/p/b.CR2');
    expect(backend.calls).toEqual([[Invokes.SetRatingForPaths, { paths: ['/p/b.CR2'], rating: 3 }]]);
    expect(useLibraryStore.getState().filterCriteria.rating).toBe(3);
  });

  it('offers Undo when removing a pick hides the photo, and Undo picks it again', async () => {
    useLibraryStore.setState({
      imageList: [
        photo('/p/a.CR2', ImageFlag.Pick),
        photo('/p/b.CR2', ImageFlag.Pick),
        photo('/p/c.CR2', ImageFlag.Pick),
      ],
    });
    showOnly({ flagStatus: FlagStatus.Picked });

    actions().handleSetFlag(null);
    expect(shown()).toEqual(['/p/a.CR2', '/p/c.CR2']);
    expect(popup.shown).toHaveLength(1);

    await settled();
    backend.calls.length = 0;
    popup.shown[0].props.onUndo();
    await settled();

    expect(shown()).toEqual(['/p/a.CR2', '/p/b.CR2', '/p/c.CR2']);
    expect(backend.calls).toEqual([[Invokes.SetFlagForPaths, { paths: ['/p/b.CR2'], flag: ImageFlag.Pick }]]);
  });

  it('counts every hidden photo when several are changed at once', () => {
    useLibraryStore.setState({ multiSelectedPaths: ['/p/a.CR2', '/p/b.CR2'] });
    showOnly({ rating: 3 });

    actions().handleRate(1);

    expect(popup.shown[0].props.count).toBe(2);
  });

  it('shows nothing without a filter', () => {
    actions().handleRate(1);
    actions().handleSetFlag(ImageFlag.Reject);

    expect(shown()).toEqual(['/p/a.CR2', '/p/b.CR2', '/p/c.CR2']);
    expect(popup.shown).toHaveLength(0);
  });

  it('shows nothing when the photo still matches the filter', () => {
    showOnly({ rating: 3 });

    actions().handleRate(5);

    expect(shown()).toContain('/p/b.CR2');
    expect(popup.shown).toHaveLength(0);
  });

  it('sends the reject flag again after restoring a rating, which clears it on disk', async () => {
    useLibraryStore.setState({
      imageList: [photo('/p/a.CR2'), photo('/p/b.CR2', ImageFlag.Reject), photo('/p/c.CR2')],
    });
    showOnly({ rating: 3 });

    actions().handleRate(0);
    await settled();
    backend.calls.length = 0;
    popup.shown[0].props.onUndo();
    await settled();

    expect(backend.calls).toEqual([
      [Invokes.SetRatingForPaths, { paths: ['/p/b.CR2'], rating: 3 }],
      [Invokes.SetFlagForPaths, { paths: ['/p/b.CR2'], flag: ImageFlag.Reject }],
    ]);
  });
});
