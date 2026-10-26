/**
 * Undo for a rating, flag or colour label that hides photos under the library's filter. Ours.
 *
 * `hooks/useLibraryActions.ts` takes a snapshot of the photos it is about to change; when the
 * change hides them, it shows a pop-up whose Undo puts the snapshot back, in the store and on
 * disk. The filter itself is left as it is. From RapidRAW 1.6.7 (4eb6b2da, 18ebf030).
 */

import { createElement } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { Invokes, ImageFile, ImageFlag } from '../components/ui/AppProperties';
import FilterExitToast from './FilterExitToast';

// RapidRAW 4eb6b2da: src/hooks/useLibraryActions.ts
export interface MetadataSnapshot {
  rating: number;
  flag: ImageFlag | null;
  color: string | null;
}

export const FILTER_EXIT_TOAST_ID = 'filter-exit';

const getColorLabel = (image?: ImageFile) => image?.tags?.find((tag) => tag.startsWith('color:'))?.substring(6) ?? null;

export const snapshotMetadata = (paths: string[]) => {
  const { imageList, imageRatings } = useLibraryStore.getState();
  const imagesByPath = new Map(imageList.map((image) => [image.path, image]));
  return new Map<string, MetadataSnapshot>(
    paths.map((path) => {
      const image = imagesByPath.get(path);
      return [path, { rating: imageRatings[path] || 0, flag: image?.flag ?? null, color: getColorLabel(image) }];
    }),
  );
};

export const restoreMetadata = (snapshot: Map<string, MetadataSnapshot>) => {
  const current = snapshotMetadata([...snapshot.keys()]);
  // RapidRAW 18ebf030: src/hooks/useLibraryActions.ts
  const groupChanged = <K extends keyof MetadataSnapshot>(
    field: K,
    force?: (path: string, before: MetadataSnapshot) => boolean,
  ) => {
    const groups = new Map<MetadataSnapshot[K], string[]>();
    snapshot.forEach((before, path) => {
      if (current.get(path)?.[field] === before[field] && !force?.(path, before)) return;
      groups.set(before[field], [...(groups.get(before[field]) ?? []), path]);
    });
    return groups;
  };

  const ratingGroups = groupChanged('rating');
  const restoredRatingPaths = new Set([...ratingGroups.values()].flat());
  // Restoring a rating > 0 clears a reject flag in the backend, so re-send the reject afterwards.
  const flagGroups = groupChanged(
    'flag',
    (path, before) => before.flag === ImageFlag.Reject && before.rating > 0 && restoredRatingPaths.has(path),
  );
  const colorGroups = groupChanged('color');

  // RapidRAW 4eb6b2da: src/hooks/useLibraryActions.ts
  useLibraryStore.getState().setLibrary((state) => {
    const imageRatings = { ...state.imageRatings };
    ratingGroups.forEach((paths, rating) => paths.forEach((path) => (imageRatings[path] = rating)));
    const flagPaths = new Set([...flagGroups.values()].flat());
    const colorPaths = new Set([...colorGroups.values()].flat());
    return {
      imageRatings,
      imageList: state.imageList.map((image) => {
        const before = snapshot.get(image.path);
        if (!before || (!flagPaths.has(image.path) && !colorPaths.has(image.path))) return image;
        const otherTags = (image.tags || []).filter((tag) => !tag.startsWith('color:'));
        const restoredTags = before.color ? [...otherTags, `color:${before.color}`] : otherTags;
        return { ...image, flag: before.flag, tags: colorPaths.has(image.path) ? restoredTags : image.tags };
      }),
    };
  });

  (async () => {
    for (const [rating, paths] of ratingGroups) await invoke(Invokes.SetRatingForPaths, { paths, rating });
    for (const [flag, paths] of flagGroups) await invoke(Invokes.SetFlagForPaths, { paths, flag });
    for (const [color, paths] of colorGroups) await invoke(Invokes.SetColorLabelForPaths, { paths, color });
  })().catch((err) => {
    console.error(err);
    toast.error(`Failed to undo: ${err}`);
  });
};

export const showFilterExitToast = (
  activePath: string,
  hiddenCount: number,
  snapshot: Map<string, MetadataSnapshot>,
  multiSelectedPaths: string[],
) => {
  const undo = () => {
    toast.dismiss(FILTER_EXIT_TOAST_ID);
    restoreMetadata(snapshot);
    if (!useLibraryStore.getState().imageList.some((image) => image.path === activePath)) return;
    useLibraryStore.getState().setLibrary({
      multiSelectedPaths: multiSelectedPaths.includes(activePath) ? multiSelectedPaths : [activePath],
      libraryActivePath: activePath,
      selectionAnchorPath: activePath,
    });
    useUIStore.getState().imageSelectHandler?.(activePath, useUIStore.getState().activeView === 'editor');
  };

  const content = createElement(FilterExitToast, {
    fileName: activePath.split('?vc=')[0].split(/[\\/]/).pop() || '',
    count: hiddenCount,
    onUndo: undo,
  });

  if (toast.isActive(FILTER_EXIT_TOAST_ID)) {
    toast.update(FILTER_EXIT_TOAST_ID, { render: content, autoClose: 5000 });
  } else {
    toast(content, { toastId: FILTER_EXIT_TOAST_ID });
  }
};
