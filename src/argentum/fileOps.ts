/**
 * What the library shows after a delete or a rename. Ours.
 *
 * Each rule is one decision `hooks/useFileOperations.ts` used to make inline;
 * they follow RapidRAW 1.6.7's fixes to that file (3c0a0082).
 */

const VIRTUAL_COPY = '?vc=';

const photoOf = (path: string) => path.split(VIRTUAL_COPY)[0];

/** Whether `path` goes when `deleted` are deleted: deleting a photo deletes its virtual copies too. */
export const isDeletedWith = (path: string, deleted: readonly string[]): boolean =>
  deleted.includes(path) || deleted.includes(photoOf(path));

/**
 * The photo to make active once `deleted` are deleted, with `order` the library as shown. The active
 * photo if it stays; else the next one that stays, else the previous one; else none.
 */
// RapidRAW 3c0a0082: src/hooks/useFileOperations.ts (virtual copies of a deleted photo are skipped)
export function activePathAfterDelete(
  order: readonly string[],
  activePath: string,
  deleted: readonly string[],
): string | null {
  if (!isDeletedWith(activePath, deleted)) return activePath;
  const index = order.indexOf(activePath);
  if (index === -1) return null;
  const stays = (path: string) => !isDeletedWith(path, deleted);
  return order.slice(index + 1).find(stays) ?? order.slice(0, index).reverse().find(stays) ?? null;
}

/** Whether `path` is the folder `folder` or inside it. `/photos/trip2` is not inside `/photos/trip`. */
export const isInFolder = (path: string, folder: string): boolean =>
  path === folder || path.startsWith(`${folder}/`) || path.startsWith(`${folder}\\`);

/** `path` once the folder `oldPath` is renamed to `newPath`: inside it, it moves with it; elsewhere it stays. */
// RapidRAW 3c0a0082: src/hooks/useFileOperations.ts (folder paths matched by folder, not by prefix)
export const pathAfterFolderRename = (path: string, oldPath: string, newPath: string): string =>
  isInFolder(path, oldPath) ? newPath + path.slice(oldPath.length) : path;

/** What happens to the photo on show when it is renamed. */
export type ShownAfterRename = { select: string; openInEditor: boolean } | 'backToLibrary' | 'nothing';

/**
 * The renamed photo on show stays on show under `newPath`, in the editor only if it was open there.
 * Without a new path, the editor goes back to the grid; the grid has nothing to do.
 */
// RapidRAW 3c0a0082: src/hooks/useFileOperations.ts (renaming in the grid no longer opens the editor)
export function shownAfterRename(newPath: string | undefined, inEditor: boolean): ShownAfterRename {
  if (newPath) return { select: newPath, openInEditor: inEditor };
  return inEditor ? 'backToLibrary' : 'nothing';
}
