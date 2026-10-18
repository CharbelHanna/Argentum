/**
 * The Modern window's folder column: where it opens, the disks, the recent
 * folders. Ours.
 *
 * There is no start screen. Where today's library shows its splash until a
 * folder is added or a session continued, the Modern window asks the backend
 * where to open (mods/modern_start.rs) and fills the column at once: every
 * disk under This computer in place of folders added one by one, the recent
 * folders, and the folder last open, selected, if it is still there. The
 * folders added in Classic stay saved for Classic; Modern never changes them.
 */

import { invoke } from '@tauri-apps/api/core';
import { create } from 'zustand';
import { ag } from '../ag';
import { forgetFolderCounts } from './folderCounts';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { useEditorStore } from '../../store/useEditorStore';
import { useProcessStore } from '../../store/useProcessStore';
import { Invokes, type ImageFile } from '../../components/ui/AppProperties';

/** A disk as mods/drives.rs lists it. Hand-written until the contract is generated. */
export interface Drive {
  path: string;
  label: string;
  removable: boolean;
  /** Zero where the system does not report it. */
  freeBytes: number;
  totalBytes: number;
}

/** mods/modern_start.rs `ModernStart`. Hand-written until the contract is generated. */
interface ModernStart {
  drives: Drive[];
  recent: string[];
  startFolder: string | null;
  expanded: string[];
}

interface FoldersState {
  drives: Drive[];
  /** Newest first, only folders that are there. */
  recent: string[];
  /** No disk could be listed, so today's start screen shows instead. */
  unavailable: boolean;
}

export const useModernFolders = create<FoldersState>(() => ({ drives: [], recent: [], unavailable: false }));

/** How the library writes an album in `currentFolderPath`. */
const ALBUM_PREFIX = 'Album: ';

/** The Modern window is the only arrangement; see useModernLayout. */
export const isModern = () => true;

/**
 * Whether unfolding a folder counts its photos the library's way. In Modern
 * the tree's roots are whole disks, and that count reads every folder under
 * them before the tree shows, so no: each row counts its own (folderCounts.ts).
 */
export const rootImageCounts = (wanted: boolean) => wanted && !isModern();

/** The disks and recent folders, as the backend sees them now. None if they could not be listed. */
async function listDisks(lastFolder: string | null, expanded: string[]): Promise<ModernStart | null> {
  try {
    const start = await ag<ModernStart>('modern_start', { lastFolder, expanded });
    useModernFolders.setState({ drives: start.drives, recent: start.recent, unavailable: start.drives.length === 0 });
    return start;
  } catch (err) {
    console.error('Could not list the disks:', err);
    useModernFolders.setState({ unavailable: true });
    return null;
  }
}

/** What App knows about the first folder, read ahead while the app starts (useAppInitialization). */
interface Preloaded {
  currentPath?: string;
  images?: Promise<ImageFile[]>;
}

type SelectFolder = (
  path: string,
  isNewRoot?: boolean,
  preloadedImages?: ImageFile[],
  expandParents?: boolean,
  preserveEditor?: boolean,
) => Promise<void>;

/**
 * The Modern window's stand-in for Continue Session: fill the column, then
 * open the folder last open, with its photos read ahead if they are of that
 * folder. A photo opened with Argentum stays open; the folder fills in behind
 * it. If no disk can be listed nothing changes, and today's start screen shows.
 */
export function startModernLibrary(preloaded: { current: Preloaded | null }, selectFolder: SelectFolder): void {
  const run = async () => {
    const { appSettings } = useSettingsStore.getState();
    const { setLibrary } = useLibraryStore.getState();
    const saved = appSettings?.lastFolderState;
    const start = await listDisks(saved?.currentFolderPath ?? null, saved?.expandedFolders ?? []);
    const roots = start?.drives.map((d) => d.path) ?? [];
    if (!start || roots.length === 0) return;
    watchLibrary();

    setLibrary({ rootPaths: roots, expandedFolders: new Set(start.expanded), isTreeLoading: true });
    try {
      const trees = await invoke<unknown[]>(Invokes.GetPinnedFolderTrees, {
        paths: roots,
        expandedFolders: start.expanded,
        showImageCounts: false,
      });
      setLibrary({ folderTrees: trees });
    } catch (err) {
      console.error('Could not read the disks for the folder column:', err);
    } finally {
      setLibrary({ isTreeLoading: false });
    }

    if (!start.startFolder) return;
    const ahead = preloaded.current;
    let images: ImageFile[] | undefined;
    if (ahead?.currentPath === start.startFolder && ahead.images) {
      images = await ahead.images.catch(() => undefined);
      ahead.images = undefined;
    }
    const { initialFileToOpen, externalEditSession } = useProcessStore.getState();
    const keepEditor = !!(useEditorStore.getState().selectedImage || initialFileToOpen || externalEditSession);
    await selectFolder(start.startFolder, false, images, false, keepEditor);
  };
  run().catch((err) => console.error('Failed to open the Modern folder column:', err));
}

/**
 * Read the column again: the disks (one plugged in since shows up) and the
 * favourites, without the library's photo counts; the rows count again
 * themselves (folderCounts.ts). Modern's stand-in for the library's own
 * refresh, which would count every folder on a disk.
 */
export async function refreshModernTrees(): Promise<void> {
  forgetFolderCounts();
  const { appSettings } = useSettingsStore.getState();
  const { expandedFolders, setLibrary } = useLibraryStore.getState();
  const expanded = Array.from(expandedFolders);
  const pinned = appSettings?.pinnedFolders ?? [];
  const start = await listDisks(null, []);
  if (!start || start.drives.length === 0) return;
  const roots = start.drives.map((d) => d.path);
  try {
    const [folderTrees, pinnedFolderTrees] = await Promise.all([
      invoke<unknown[]>(Invokes.GetPinnedFolderTrees, {
        paths: roots,
        expandedFolders: expanded,
        showImageCounts: false,
      }),
      pinned.length > 0
        ? invoke<unknown[]>(Invokes.GetPinnedFolderTrees, {
            paths: pinned,
            expandedFolders: expanded,
            showImageCounts: false,
          })
        : Promise.resolve([]),
    ]);
    setLibrary({ rootPaths: roots, folderTrees, pinnedFolderTrees });
  } catch (err) {
    console.error('Failed to refresh the folder column:', err);
  }
}

let watching = false;

/**
 * While Modern is chosen: every folder opened goes to the top of Recent, and
 * the tree's roots stay the disks. The library still loads the folders added
 * in Classic when its photo-count setting changes; if that or anything else
 * puts them in the tree, the disks are read back.
 */
function watchLibrary() {
  if (watching) return;
  watching = true;
  useLibraryStore.subscribe((state, previous) => {
    if (!isModern()) return;
    const folder = state.currentFolderPath;
    if (folder && folder !== previous.currentFolderPath && !folder.startsWith(ALBUM_PREFIX)) {
      ag<string[]>('add_recent_folder', { path: folder })
        .then((recent) => useModernFolders.setState({ recent }))
        .catch((err) => console.warn('Recent folders not saved:', err));
    }
    if (state.folderTrees !== previous.folderTrees) {
      const disks = useModernFolders.getState().drives.map((d) => d.path);
      const roots = state.folderTrees.map((tree: { path?: string }) => tree.path);
      if (disks.length > 0 && roots.some((root) => !root || !disks.includes(root))) {
        refreshModernTrees();
      }
    }
  });
}

/** A disk as the column names it: its letter or mount point, then its own name. */
export function driveName(drive: Drive): string {
  const where = drive.path.replace(/[\\/]+$/, '') || drive.path;
  return drive.label ? `${where}  ${drive.label}` : where;
}

/** The last part of a folder's path. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/**
 * A folder's path as breadcrumbs, each with the path it opens: `E:\2024\Trip`
 * is E: (E:\), 2024 (E:\2024), Trip (E:\2024\Trip).
 */
export function crumbs(path: string): { name: string; path: string }[] {
  const sep = path.includes('\\') ? '\\' : '/';
  const parts = path.split(/[\\/]+/).filter(Boolean);
  const out: { name: string; path: string }[] = [];
  let acc = path.startsWith('\\\\') ? '\\\\' : path.startsWith('/') ? '/' : '';
  parts.forEach((part, i) => {
    acc = i === 0 ? `${acc}${part}` : `${acc}${acc.endsWith(sep) ? '' : sep}${part}`;
    // A drive letter's own folder is its root, E:\, not E:.
    const opens = i === 0 && /^[A-Za-z]:$/.test(part) ? `${part}${sep}` : acc;
    out.push({ name: part, path: opens });
  });
  return out;
}
