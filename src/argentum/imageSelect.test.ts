// Choosing photos quickly (arrow keys held down): the latest choice wins, even when an
// earlier one is still waiting on the backend. handleImageSelect in hooks/useAppNavigation.ts,
// RapidRAW ecf7730f.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const backend = vi.hoisted(() => ({
  /** Answers to is_image_cached held back, by path. */
  held: new Map<string, (cached: boolean) => void>(),
  slow: new Set<string>(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (command: string, args?: { path?: string }) => {
    const path = args?.path ?? '';
    if (command === 'is_image_cached' && backend.slow.has(path)) {
      return new Promise((resolve) => backend.held.set(path, resolve));
    }
    if (command === 'is_image_cached') return Promise.resolve(true);
    // load_image and load_metadata: never answer, their results are not what is tested here.
    return new Promise(() => {});
  },
}));

// The hook uses no React hook but useCallback, so it can be called outside a component.
vi.mock('react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('react')>()),
  useCallback: <T>(fn: T) => fn,
}));

import { useAppNavigation } from '../hooks/useAppNavigation';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { globalImageCache } from '../utils/ImageLRUCache';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';

/** A photo opened before, so its settings and preview are kept in the frontend. */
const openedBefore = (path: string) =>
  globalImageCache.set(path, {
    adjustments: INITIAL_ADJUSTMENTS,
    histogram: null,
    waveform: null,
    finalPreviewUrl: null,
    uncroppedPreviewUrl: null,
    selectedImage: { path, isReady: true },
    originalSize: { width: 6000, height: 4000 },
    previewSize: { width: 1500, height: 1000 },
  });

const ref = <T>(current: T) => ({ current });
const selectedImagePathRef = ref<string | null>(null);
const navigation = () =>
  useAppNavigation({
    clearThumbnailQueue: () => {},
    refs: {
      transformWrapperRef: ref(null),
      preloadedDataRef: ref(null),
      cachedEditStateRef: ref(null),
      selectedImagePathRef,
      isBackendReadyRef: ref(false),
      latestRenderedJobIdRef: ref(0),
      previewJobIdRef: ref(0),
      currentResRef: ref(0),
      prevAdjustmentsRef: ref(null),
    },
  });

describe('choosing photos quickly', () => {
  beforeEach(() => {
    backend.held.clear();
    backend.slow.clear();
    useEditorStore.setState({ selectedImage: null });
    useLibraryStore.setState({ libraryActivePath: null, multiSelectedPaths: [] });
  });

  it('keeps the photo chosen last when an earlier choice answers late', async () => {
    openedBefore('a.cr3');
    openedBefore('b.cr3');
    backend.slow.add('a.cr3');
    const { handleImageSelect } = navigation();

    const first = handleImageSelect('a.cr3', false);
    await handleImageSelect('b.cr3', false);
    backend.held.get('a.cr3')?.(true);
    await first;

    expect(useLibraryStore.getState().libraryActivePath).toBe('b.cr3');
    expect(useLibraryStore.getState().multiSelectedPaths).toEqual(['b.cr3']);
    expect(useEditorStore.getState().selectedImage?.path).toBe('b.cr3');
    expect(selectedImagePathRef.current).toBe('b.cr3');
  });

  it('still selects a photo that answers late when nothing was chosen after it', async () => {
    openedBefore('c.cr3');
    backend.slow.add('c.cr3');
    const { handleImageSelect } = navigation();

    const only = handleImageSelect('c.cr3', false);
    backend.held.get('c.cr3')?.(true);
    await only;

    expect(useLibraryStore.getState().libraryActivePath).toBe('c.cr3');
    expect(useEditorStore.getState().selectedImage?.path).toBe('c.cr3');
  });
});
