/**
 * The Looks tab: presets, LUTs and film looks in one list. Ours.
 *
 * Search, Import, and chips for All, Favourites, Film, Presets and LUTs; then
 * what is on the photo, with a LUT's Strength and a preset's Amount; then the
 * groups: Film (the app's film LUTs), Presets (those that come with the app,
 * RAWmakase's, by group), My presets and My LUTs. Pointing at a look shows it
 * on the photo; a click puts it on, a click on the one on takes it off; the
 * star keeps it in Favourites.
 *
 * Every part is today's: a LUT goes on through their handleLutSelect and
 * previews through their setLutPreviewOverride, swatches come from their
 * generate_lut_previews and generate_preset_preview, the user's presets from
 * their usePresets and import from their commands. A preset's Amount follows
 * presetAmount.ts. The Effects section's LUT block and today's presets panel
 * did these separately; the Modern window shows this instead of both.
 */

import { type MouseEvent, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { ChevronDown, ChevronRight, Download, MoreHorizontal, Search, Star, X } from 'lucide-react';
import clsx from 'clsx';
import { toast } from 'react-toastify';
import Slider from '../../components/ui/Slider';
import { useContextMenu } from '../../context/ContextMenuContext';
import { useEditorActions } from '../../hooks/useEditorActions';
import { usePresets, type UserPreset } from '../../hooks/usePresets';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';
import { type Adjustments, INITIAL_ADJUSTMENTS } from '../../utils/adjustments';
import { type AgKey, useAgTranslation } from '../locales';
import { presetAt } from './presetAmount';
import { type LutEntry, useLooks } from './looksStore';

type Kind = 'film' | 'preset' | 'lut';
type Chip = 'all' | 'favourites' | 'film' | 'presets' | 'luts';

interface Look {
  id: string;
  name: string;
  kind: Kind;
  lut?: LutEntry;
  preset?: Partial<Adjustments>;
  /** A preset of the user's, which can be deleted. */
  own?: boolean;
}

interface Group {
  id: string;
  name: string;
  looks: Look[];
  folders?: Group[];
}

const CHIPS: { id: Chip; label: AgKey }[] = [
  { id: 'all', label: 'looksAll' },
  { id: 'favourites', label: 'looksFavourites' },
  { id: 'film', label: 'looksFilm' },
  { id: 'presets', label: 'looksPresets' },
  { id: 'luts', label: 'looksLuts' },
];

const KIND_LABEL: Record<Kind, AgKey> = { film: 'looksKindFilm', preset: 'looksKindPreset', lut: 'looksKindLut' };

const LUT_FILES = ['cube', '3dl', 'png', 'jpg', 'jpeg', 'tiff'];
const PRESET_FILES = ['rrpreset', 'xmp', 'lrtemplate'];

/** How long the pointer rests on a look before the photo shows it. */
const PREVIEW_AFTER_MS = 120;
const SWATCH_PX = 72;
const UNCATEGORIZED = 'uncategorized';

const lutLook = (entry: LutEntry): Look => ({
  id: entry.path,
  name: entry.name.replace(/\.[^.]+$/, ''),
  kind: entry.isBuiltIn ? 'film' : 'lut',
  lut: entry,
});

function userGroups(items: UserPreset[]): { root: Look[]; folders: Group[] } {
  const look = (item: UserPreset): Look | null =>
    item.preset
      ? { id: item.preset.id, name: item.preset.name, kind: 'preset', preset: item.preset.adjustments, own: true }
      : null;
  const root = items.map(look).filter((l): l is Look => l !== null);
  const folders = items
    .filter((item) => item.folder)
    .map((item) => ({
      id: `folder:${item.folder?.id}`,
      name: item.folder?.name ?? '',
      looks: ((item.folder?.children ?? []) as UserPreset[]).map(look).filter((l): l is Look => l !== null),
    }));
  return { root, folders };
}

export default function LooksPanel({ editing }: { editing: boolean }) {
  const t = useAgTranslation();
  const { showContextMenu } = useContextMenu();
  const { setAdjustments, handleLutSelect, setLutPreviewOverride } = useEditorActions();
  const adjustments = useEditorStore((s) => s.adjustments);
  const photo = useEditorStore((s) => (editing ? s.selectedImage : null));
  const setEditor = useEditorStore((s) => s.setEditor);
  const { builtin, luts, libraries, favourites, applied, load, refreshLuts, toggleFavourite, setApplied } = useLooks();
  const { presets: userItems, importPresetsFromFiles, deleteItem } = usePresets(adjustments);
  const [chip, setChip] = useState<Chip>('all');
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<Set<string>>(() => new Set(['film', 'presets', 'mine', 'myluts']));
  const hover = useRef<number | undefined>(undefined);

  useEffect(() => {
    load();
  }, [load]);

  // A preset on another photo is not this photo's.
  const onPhoto = applied && photo && applied.photo === photo.path ? applied : null;
  useEffect(() => {
    if (applied && photo && applied.photo !== photo.path) setApplied(null);
  }, [applied, photo, setApplied]);

  useEffect(() => () => window.clearTimeout(hover.current), []);

  const sections = useMemo(() => {
    const mine = userGroups(userItems);
    const film: Group = { id: 'film', name: t('looksGroupFilm'), looks: luts.filter((l) => l.isBuiltIn).map(lutLook) };
    const presets: Group = {
      id: 'presets',
      name: t('looksGroupPresets'),
      looks: [],
      folders: builtin.map((group) => ({
        id: `builtin:${group.name}`,
        name: group.name,
        looks: group.presets.map((p) => ({ id: p.id, name: p.name, kind: 'preset' as const, preset: p.adjustments })),
      })),
    };
    const own: Group = { id: 'mine', name: t('looksGroupMine'), looks: mine.root, folders: mine.folders };
    const myLuts: Group = {
      id: 'myluts',
      name: t('looksGroupMyLuts'),
      looks: [],
      folders: [{ id: UNCATEGORIZED, name: t('looksImported') }, ...libraries]
        .map((library) => ({
          id: `lib:${library.id}`,
          name: library.name,
          looks: luts.filter((l) => !l.isBuiltIn && (l.libraryId ?? UNCATEGORIZED) === library.id).map(lutLook),
        }))
        .filter((group) => group.looks.length > 0),
    };
    return { film, presets, own, myLuts };
  }, [builtin, luts, libraries, userItems, t]);

  const everyLook = useMemo(() => {
    const flat = (g: Group): Look[] => [...g.looks, ...(g.folders ?? []).flatMap(flat)];
    return [sections.film, sections.presets, sections.own, sections.myLuts].flatMap(flat);
  }, [sections]);

  // ------------------------------------------------------------- actions

  const isOn = (look: Look) => (look.lut ? adjustments.lutPath === look.lut.path : onPhoto?.id === look.id);

  const stopPreview = () => {
    window.clearTimeout(hover.current);
    setEditor({ previewOverride: null });
  };

  const startPreview = (look: Look) => {
    if (!photo) return;
    window.clearTimeout(hover.current);
    hover.current = window.setTimeout(() => {
      if (look.lut) {
        setLutPreviewOverride(look.lut.path, look.lut.isBuiltIn);
      } else if (look.preset) {
        const now = useEditorStore.getState().adjustments;
        setEditor({ previewOverride: { ...now, ...presetAt(now, look.preset, 100) } });
      }
    }, PREVIEW_AFTER_MS);
  };

  const clearLut = () =>
    setAdjustments((prev: Adjustments) => ({
      ...prev,
      lutPath: null,
      lutName: null,
      lutData: null,
      lutSize: 0,
      lutIntensity: 100,
      lutIsSceneReferred: false,
    }));

  const takeOffPreset = () => {
    if (!onPhoto) return;
    setAdjustments((prev: Adjustments) => ({ ...prev, ...presetAt(onPhoto.before, onPhoto.adjustments, 0) }));
    setApplied(null);
  };

  const choose = (look: Look) => {
    stopPreview();
    if (!photo) {
      toast.info(t('looksHintGrid'));
      return;
    }
    if (look.lut) {
      if (isOn(look)) clearLut();
      else handleLutSelect(look.lut.path, look.lut.isBuiltIn);
      return;
    }
    if (!look.preset) return;
    if (isOn(look)) {
      takeOffPreset();
      return;
    }
    // One preset at a time: the one on comes off before the next goes on.
    const now = useEditorStore.getState().adjustments;
    const before = onPhoto ? { ...now, ...presetAt(onPhoto.before, onPhoto.adjustments, 0) } : now;
    setAdjustments(() => ({ ...before, ...presetAt(before, look.preset as Partial<Adjustments>, 100) }));
    setApplied({ photo: photo.path, id: look.id, name: look.name, adjustments: look.preset, before, amount: 100 });
  };

  const changeAmount = (amount: number) => {
    if (!onPhoto) return;
    setAdjustments((prev: Adjustments) => ({ ...prev, ...presetAt(onPhoto.before, onPhoto.adjustments, amount) }));
    setApplied({ ...onPhoto, amount });
  };

  const importLooks = async () => {
    const chosen = await openDialog({
      multiple: true,
      title: t('looksImportTip'),
      filters: [
        { name: t('looksImportTip'), extensions: [...LUT_FILES, ...PRESET_FILES] },
        { name: t('looksLuts'), extensions: LUT_FILES },
        { name: t('looksPresets'), extensions: PRESET_FILES },
      ],
    });
    const paths = Array.isArray(chosen) ? chosen : chosen ? [chosen] : [];
    const ext = (p: string) => p.split('.').pop()?.toLowerCase() ?? '';
    const lutPaths = paths.filter((p) => LUT_FILES.includes(ext(p)));
    const presetPaths = paths.filter((p) => PRESET_FILES.includes(ext(p)));
    try {
      if (lutPaths.length) {
        await invoke('import_luts', { sourcePaths: lutPaths, libraryId: UNCATEGORIZED });
        await refreshLuts();
      }
      if (presetPaths.length) {
        const { failures } = await importPresetsFromFiles(presetPaths);
        if (failures.length) toast.error(t('looksImportFailed'));
      }
    } catch (err) {
      console.error('Looks: import failed:', err);
      toast.error(t('looksImportFailed'));
    }
  };

  const lookMenu = (event: MouseEvent, look: Look) => {
    event.preventDefault();
    const starred = favourites.includes(look.id);
    const options: { label: string; onClick: () => void; isDestructive?: boolean }[] = [
      { label: t(starred ? 'looksUnstar' : 'looksStar'), onClick: () => toggleFavourite(look.id) },
    ];
    if (look.own) {
      options.push({ label: t('looksDelete'), isDestructive: true, onClick: () => deleteItem(look.id) });
    }
    if (look.kind === 'lut' && look.lut) {
      const path = look.lut.path;
      options.push({
        label: t('looksDelete'),
        isDestructive: true,
        onClick: () =>
          invoke('remove_lut', { path })
            .then(refreshLuts)
            .catch(() => toast.error(String(path))),
      });
    }
    showContextMenu(event.clientX, event.clientY, options);
  };

  const moreMenu = (event: MouseEvent) => {
    const rect = event.currentTarget.getBoundingClientRect();
    showContextMenu(rect.left, rect.bottom, [
      { label: t('looksCommunity'), onClick: () => useUIStore.getState().setUI({ activeView: 'community' }) },
    ]);
  };

  // ------------------------------------------------------------- what shows

  const needle = query.trim().toLowerCase();
  const matches = (look: Look) => !needle || look.name.toLowerCase().includes(needle);
  const flatList = chip === 'favourites' || !!needle;
  const shownFlat = flatList
    ? everyLook.filter((l) => matches(l) && (chip !== 'favourites' || favourites.includes(l.id)) && inChip(l, chip))
    : [];
  const shownSections = flatList
    ? []
    : chip === 'film'
      ? [sections.film]
      : chip === 'presets'
        ? [sections.presets, sections.own]
        : chip === 'luts'
          ? [sections.myLuts]
          : [sections.film, sections.presets, sections.own, sections.myLuts];

  // Swatches only for what is on screen: a group asks for its own when it opens.
  const visible = flatList
    ? shownFlat
    : shownSections.flatMap((section) =>
        open.has(section.id)
          ? [...section.looks, ...(section.folders ?? []).flatMap((f) => (open.has(f.id) ? f.looks : []))]
          : [],
      );
  const onPhotoLooks = everyLook.filter((l) => l.id === adjustments.lutPath || l.id === onPhoto?.id);
  const swatches = useSwatches([...visible, ...onPhotoLooks], photo?.path ?? null, !!photo?.isReady);

  const toggle = (id: string) =>
    setOpen((was) => {
      const next = new Set(was);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const row = (look: Look, showKind = false) => (
    <div
      key={look.id}
      className={clsx('agm-look', isOn(look) && 'agm-on')}
      onClick={() => choose(look)}
      onMouseEnter={() => startPreview(look)}
      onMouseLeave={stopPreview}
      onContextMenu={(e) => lookMenu(e, look)}
    >
      <span
        className="agm-swatch"
        style={swatches[look.id] ? { backgroundImage: `url(${swatches[look.id]})` } : undefined}
      />
      <span className="agm-look-name">{look.name}</span>
      {showKind && <span className="agm-kind">{t(KIND_LABEL[look.kind])}</span>}
      <button
        className={clsx('agm-star', favourites.includes(look.id) && 'agm-on')}
        onClick={(e) => {
          e.stopPropagation();
          toggleFavourite(look.id);
        }}
        data-tooltip={t(favourites.includes(look.id) ? 'looksUnstar' : 'looksStar')}
      >
        <Star size={12} />
      </button>
    </div>
  );

  const groupHead = (group: Group, count: number, folder = false) => (
    <div className={folder ? 'agm-look-folder' : 'agm-sec-head'} onClick={() => toggle(group.id)}>
      {open.has(group.id) ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
      <span className="min-w-0 truncate">{group.name}</span>
      <span className="agm-count">{count}</span>
    </div>
  );

  const countOf = (g: Group): number => g.looks.length + (g.folders ?? []).reduce((n, f) => n + countOf(f), 0);

  const lutOn = adjustments.lutPath ? luts.find((l) => l.path === adjustments.lutPath) : undefined;

  return (
    <div className="agm-looks">
      <div className="agm-looks-head">
        <div className="agm-looks-row">
          <label className="agm-search">
            <Search size={13} />
            <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t('looksSearch')} />
            {query && (
              <button onClick={() => setQuery('')} aria-label={t('looksClear')}>
                <X size={12} />
              </button>
            )}
          </label>
          <button className="agm-btn agm-btn-box" onClick={importLooks} data-tooltip={t('looksImportTip')}>
            <Download size={13} />
            {t('looksImport')}
          </button>
          <button className="agm-btn" onClick={moreMenu} data-tooltip={t('looksMore')}>
            <MoreHorizontal size={14} />
          </button>
        </div>
        <div className="agm-chips" role="tablist">
          {CHIPS.map((c) => (
            <button
              key={c.id}
              role="tab"
              aria-selected={chip === c.id}
              className={clsx('agm-chip', chip === c.id && 'agm-on')}
              onClick={() => setChip(c.id)}
            >
              {c.id === 'favourites' && <Star size={11} />}
              {t(c.label)}
            </button>
          ))}
        </div>
        <div className="agm-hint">{t(photo ? 'looksHintPhoto' : 'looksHintGrid')}</div>
      </div>

      {photo && (lutOn || adjustments.lutPath || onPhoto) && (
        <div className="agm-onphoto">
          <div className="agm-sec-head">{t('looksOnPhoto')}</div>
          {adjustments.lutPath && (
            <div className="agm-onphoto-item">
              <div className="agm-look agm-on">
                <span
                  className="agm-swatch"
                  style={
                    swatches[adjustments.lutPath]
                      ? { backgroundImage: `url(${swatches[adjustments.lutPath]})` }
                      : undefined
                  }
                />
                <span className="agm-look-name">{(adjustments.lutName ?? '').replace(/\.[^.]+$/, '')}</span>
                <span className="agm-kind">{t(lutOn?.isBuiltIn ? 'looksKindFilm' : 'looksKindLut')}</span>
                <button className="agm-btn" onClick={clearLut} data-tooltip={t('looksTakeOff')}>
                  <X size={12} />
                </button>
              </div>
              <Slider
                label={t('looksStrength')}
                min={0}
                max={100}
                step={1}
                defaultValue={100}
                value={adjustments.lutIntensity ?? 100}
                onChange={(e) =>
                  setAdjustments((prev: Adjustments) => ({ ...prev, lutIntensity: Number(e.target.value) }))
                }
              />
            </div>
          )}
          {onPhoto && (
            <div className="agm-onphoto-item">
              <div className="agm-look agm-on">
                <span
                  className="agm-swatch"
                  style={swatches[onPhoto.id] ? { backgroundImage: `url(${swatches[onPhoto.id]})` } : undefined}
                />
                <span className="agm-look-name">{onPhoto.name}</span>
                <span className="agm-kind">{t('looksKindPreset')}</span>
                <button className="agm-btn" onClick={takeOffPreset} data-tooltip={t('looksTakeOff')}>
                  <X size={12} />
                </button>
              </div>
              <Slider
                label={t('looksAmount')}
                min={0}
                max={200}
                step={1}
                defaultValue={100}
                value={onPhoto.amount}
                suffix="%"
                onChange={(e) => changeAmount(Number(e.target.value))}
              />
            </div>
          )}
        </div>
      )}

      <div className="agm-looks-list">
        {flatList ? (
          shownFlat.length ? (
            shownFlat.map((look) => row(look, true))
          ) : (
            <div className="agm-hint pt-3">
              {t(chip === 'favourites' && !needle ? 'looksNoFavourites' : 'looksNothing')}
            </div>
          )
        ) : (
          shownSections.map((section) => (
            <div key={section.id} className="agm-look-section">
              {groupHead(section, countOf(section))}
              {open.has(section.id) && (
                <>
                  {(section.folders ?? []).map((folder) => (
                    <div key={folder.id}>
                      {groupHead(folder, folder.looks.length, true)}
                      {open.has(folder.id) && <div className="agm-look-inner">{folder.looks.map((l) => row(l))}</div>}
                    </div>
                  ))}
                  {section.looks.map((l) => row(l))}
                </>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function inChip(look: Look, chip: Chip): boolean {
  if (chip === 'film') return look.kind === 'film';
  if (chip === 'presets') return look.kind === 'preset';
  if (chip === 'luts') return look.kind === 'lut';
  return true;
}

/**
 * Swatches for the looks in open groups, on the open photo: LUTs through their
 * generate_lut_previews in one call, presets one at a time through their
 * generate_preset_preview, as today's presets panel does, on the app's
 * defaults. Kept for the photo until another one opens.
 */
function useSwatches(looks: Look[], photoPath: string | null, ready: boolean) {
  const [swatches, setSwatches] = useState<Record<string, string>>({});
  const forPhoto = useRef<string | null>(null);
  const asked = useRef<Set<string>>(new Set());
  const key = looks.map((l) => l.id).join('|');

  useEffect(() => {
    if (forPhoto.current !== photoPath) {
      forPhoto.current = photoPath;
      asked.current = new Set();
      setSwatches((was) => {
        Object.values(was).forEach((url) => url.startsWith('blob:') && URL.revokeObjectURL(url));
        return {};
      });
    }
  }, [photoPath]);

  useEffect(() => {
    if (!photoPath || !ready) return;
    const path = photoPath;
    const luts = looks.filter((l) => l.lut && !asked.current.has(l.id));
    const presets = looks.filter((l) => l.preset && !asked.current.has(l.id));
    luts.forEach((l) => asked.current.add(l.id));
    if (luts.length) {
      invoke<{ path: string; thumb: string | null }[]>('generate_lut_previews', {
        luts: luts.map((l) => ({ path: l.lut?.path, isBuiltIn: l.lut?.isBuiltIn })),
        size: SWATCH_PX,
      })
        .then((results) => {
          if (forPhoto.current !== path) return;
          setSwatches((was) => {
            const next = { ...was };
            results.forEach((r) => r.thumb && (next[r.path] = r.thumb));
            return next;
          });
        })
        .catch((err) => console.warn('Looks: LUT swatches:', err));
    }
    let stopped = false;
    (async () => {
      for (const look of presets) {
        if (stopped || forPhoto.current !== path) return;
        asked.current.add(look.id);
        try {
          const bytes: Uint8Array = await invoke('generate_preset_preview', {
            jsAdjustments: { ...INITIAL_ADJUSTMENTS, ...look.preset },
          });
          if (forPhoto.current !== path) return;
          const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }));
          setSwatches((was) => ({ ...was, [look.id]: url }));
        } catch {
          asked.current.delete(look.id);
          return;
        }
      }
    })();
    return () => {
      stopped = true;
    };
    // Keyed on which looks, not the array, which is new on every render.
  }, [key, photoPath, ready]);

  return swatches;
}
