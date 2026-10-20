/**
 * Snapshots, in the Modern window's left column above History: named states
 * of the photo's edit. Ours; the behaviour is RAWmakase's (src/app/
 * snapshots.rs @ 7c3c81a), the rows are the mockup's.
 *
 * + (or Ctrl+N) keeps the open photo as it is now and starts naming it. A
 * click puts a snapshot back, as a History step, so Undo takes it off again;
 * a double-click renames it; a right-click renames, updates it with the
 * current settings, or deletes it. Newest first, each with when it was made.
 *
 * They are kept in the photo's sidecar beside the edit (mods/snapshots.rs),
 * so undoing never takes one away. In the grid the selected photo's are shown
 * read-only, with a note that opening it restores them.
 */

import { type KeyboardEvent, type MouseEvent, useEffect, useRef, useState } from 'react';
import { Bookmark, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import clsx from 'clsx';
import { useContextMenu } from '../../context/ContextMenuContext';
import { useEditorActions } from '../../hooks/useEditorActions';
import { useEditorStore } from '../../store/useEditorStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { type Adjustments, normalizeLoadedAdjustments } from '../../utils/adjustments';
import { useEditorLayout } from '../layout/editorLayout';
import { useAgTranslation } from '../locales';
import { type Snapshot, useSnapshots } from './snapshotsStore';

/** Its id among the folded panes (editorLayout.ts panels.folded). */
export const SNAPSHOTS_PANE = 'left.snapshots';
const PANE = SNAPSHOTS_PANE;

/** Today the time, yesterday the word, before that the day. */
function when(at: number, yesterday: string): string {
  const date = new Date(at);
  const today = new Date();
  const days = Math.round(
    (new Date(today.toDateString()).getTime() - new Date(date.toDateString()).getTime()) / 86_400_000,
  );
  if (days === 0) return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  if (days === 1) return yesterday;
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** `height` is the list's, dragged from the line above it (LeftColumn.tsx). */
export default function SnapshotsSection({ editing, height }: { editing: boolean; height: number }) {
  const t = useAgTranslation();
  const { showContextMenu } = useContextMenu();
  const { setAdjustments } = useEditorActions();
  const openPath = useEditorStore((s) => (editing ? s.selectedImage?.path : undefined));
  const gridPath = useLibraryStore((s) => s.libraryActivePath);
  const path = editing ? openPath : gridPath;
  const list = useSnapshots((s) => (path ? s.byPath[path] : undefined));
  const load = useSnapshots((s) => s.load);
  const keep = useSnapshots((s) => s.keep);
  const folded = useEditorLayout((s) => s.panels.folded.includes(PANE));
  const changePanels = useEditorLayout((s) => s.changePanels);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (path) load(path);
  }, [path, load]);

  useEffect(() => {
    field.current?.select();
  }, [renaming?.id]);

  const shown = [...(list ?? [])].sort((a, b) => b.at - a.at);

  const add = () => {
    if (!editing || !path) return;
    const at = Date.now();
    const snapshot: Snapshot = {
      id: crypto.randomUUID(),
      name: new Date(at).toLocaleString(undefined, {
        day: 'numeric',
        month: 'short',
        hour: '2-digit',
        minute: '2-digit',
      }),
      at,
      adjustments: structuredClone(useEditorStore.getState().adjustments),
    };
    keep(path, [...(list ?? []), snapshot]);
    if (folded) changePanels({ folded: useEditorLayout.getState().panels.folded.filter((p) => p !== PANE) });
    setRenaming({ id: snapshot.id, name: snapshot.name });
  };

  // Ctrl+N, while a photo is open and nothing is being typed.
  useEffect(() => {
    if (!editing) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      const typing = (e.target as HTMLElement)?.closest('input, textarea, [contenteditable="true"]');
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'n' && !typing) {
        e.preventDefault();
        add();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const restore = (snapshot: Snapshot) => {
    if (!editing) return;
    setAdjustments(() => normalizeLoadedAdjustments(snapshot.adjustments as Adjustments));
  };

  const commitRename = () => {
    if (!renaming || !path) return;
    const name = renaming.name.trim();
    if (name)
      keep(
        path,
        (list ?? []).map((s) => (s.id === renaming.id ? { ...s, name } : s)),
      );
    setRenaming(null);
  };

  const menu = (event: MouseEvent, snapshot: Snapshot) => {
    event.preventDefault();
    if (!editing || !path) return;
    showContextMenu(event.clientX, event.clientY, [
      { label: t('snapRename'), onClick: () => setRenaming({ id: snapshot.id, name: snapshot.name }) },
      {
        label: t('snapUpdate'),
        onClick: () =>
          keep(
            path,
            (list ?? []).map((s) =>
              s.id === snapshot.id
                ? { ...s, at: Date.now(), adjustments: structuredClone(useEditorStore.getState().adjustments) }
                : s,
            ),
          ),
      },
      {
        label: t('snapDelete'),
        isDestructive: true,
        onClick: () =>
          keep(
            path,
            (list ?? []).filter((s) => s.id !== snapshot.id),
          ),
      },
    ]);
  };

  const onFieldKey = (e: KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === 'Enter') commitRename();
    if (e.key === 'Escape') setRenaming(null);
  };

  return (
    <div className="agm-snapshots">
      <div
        className="agm-sec-head"
        onClick={() =>
          changePanels({
            folded: folded
              ? useEditorLayout.getState().panels.folded.filter((p) => p !== PANE)
              : [...useEditorLayout.getState().panels.folded, PANE],
          })
        }
      >
        {folded ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
        {t('snapTitle')}
        <span className="agm-sec-tools">
          <button
            className="agm-btn"
            disabled={!editing}
            data-tooltip={t('snapNew')}
            onClick={(e) => {
              e.stopPropagation();
              add();
            }}
          >
            <Plus size={13} />
          </button>
        </span>
      </div>
      {!folded && (
        // agm-scroll, not custom-scrollbar: the UI benchmark scrolls the first of those, the grid (modern.css).
        <div className="agm-snapshots-body agm-scroll" style={{ height }}>
          {shown.length === 0 && editing && <div className="agm-hint px-2.5 py-1">{t('snapEmpty')}</div>}
          {shown.map((snapshot) => (
            <div
              key={snapshot.id}
              className={clsx('agm-snap', !editing && 'agm-dim')}
              onClick={() => restore(snapshot)}
              onDoubleClick={() => editing && setRenaming({ id: snapshot.id, name: snapshot.name })}
              onContextMenu={(e) => menu(e, snapshot)}
            >
              <Bookmark size={12} className="shrink-0" />
              {renaming?.id === snapshot.id ? (
                <input
                  ref={field}
                  className="agm-snap-field"
                  value={renaming.name}
                  onChange={(e) => setRenaming({ id: snapshot.id, name: e.target.value })}
                  onKeyDown={onFieldKey}
                  onBlur={commitRename}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <span className="agm-snap-name">{snapshot.name}</span>
              )}
              <span className="agm-snap-when">{when(snapshot.at, t('snapYesterday'))}</span>
            </div>
          ))}
          {shown.length > 0 && !editing && <div className="agm-hint px-2.5 py-1">{t('snapGridNote')}</div>}
        </div>
      )}
    </div>
  );
}
