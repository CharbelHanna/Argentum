/**
 * The Provider looks section of the Looks tab, and the look on the photo.
 * Ours.
 *
 * It shows only when a provider is set and the photo can have its looks, so
 * with `lutProvider` absent or empty nothing here renders. Its switch picks
 * which list is usable: on, these looks and the Camera picker work and the
 * regular Presets and LUTs are off; off, the reverse. It never takes a look off
 * a photo. A click starts a job and the photo stays as it was; the store puts
 * the look on when its file is ready.
 */

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X } from 'lucide-react';
import clsx from 'clsx';
import Slider from '../../components/ui/Slider';
import { useEditorStore } from '../../store/useEditorStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useUIStore } from '../../store/useUIStore';
import { ag } from '../ag';
import type { OfferedLook } from '../ag.generated';
import { useAgTranslation } from '../locales';
import { lookKeys, lookOf, lookOnto } from './providerLook';
import { restoreLook, useProviderLooks } from './providerLooksStore';

/** How long the pointer rests on a look before the photo shows it, as in the lists below. */
const PREVIEW_AFTER_MS = 120;
const SWATCH_PX = 72;

/** Swatches from the look files beside the open photo, by file. */
function useFileSwatches(files: { path: string }[], photo: string | null, ready: boolean) {
  const [swatches, setSwatches] = useState<Record<string, string>>({});
  const made = useProviderLooks((s) => s.filesMade);
  const key = files.map((f) => f.path).join('|');

  useEffect(() => {
    setSwatches({});
    if (!photo || !ready || !key) return;
    let current = true;
    invoke<{ path: string; thumb: string | null }[]>('generate_lut_previews', {
      luts: key.split('|').map((path) => ({ path, isBuiltIn: false, linear: true })),
      size: SWATCH_PX,
    })
      .then((results) => {
        if (current) setSwatches(Object.fromEntries(results.flatMap((r) => (r.thumb ? [[r.path, r.thumb]] : []))));
      })
      .catch((err) => console.warn('Provider looks: swatches:', err));
    return () => {
      current = false;
    };
  }, [key, photo, ready, made]);

  return swatches;
}

interface Props {
  /** The open photo; null in the grid, where a click goes to the selection. */
  photo: string | null;
  ready: boolean;
  onTakeOff: () => void;
}

export default function ProviderLooks({ photo, ready, onTakeOff }: Props) {
  const t = useAgTranslation();
  const adjustments = useEditorStore((s) => s.adjustments);
  const setEditor = useEditorStore((s) => s.setEditor);
  const gridPhoto = useLibraryStore((s) => s.libraryActivePath ?? s.multiSelectedPaths[0] ?? null);
  const subject = photo ?? gridPhoto;
  const { offer, waiting, note, batch, load, cameraOf, chooseCamera, pick, putOnSelection, cancelBatch, setOn } =
    useProviderLooks();
  const hover = useRef<number | undefined>(undefined);
  const files = offer?.photo?.files ?? [];
  const swatches = useFileSwatches(files, photo, ready);

  // A provider that fails its check is asked again when the tab next opens, not per photo.
  const available = !!offer;
  const asked = useRef(false);
  useEffect(() => {
    if (asked.current && !available) return;
    asked.current = true;
    load(subject);
  }, [subject, available, load]);

  useEffect(
    () => () => {
      window.clearTimeout(hover.current);
    },
    [],
  );

  // Opening a photo whose look has no file.
  useEffect(() => {
    if (photo && ready && offer) restoreLook(photo, useEditorStore.getState().adjustments);
  }, [photo, ready, offer]);

  // Esc lets go of a look still being made, before it does anything else.
  const spinning = !!photo && !!waiting[photo]?.spinning;
  useEffect(() => {
    if (!spinning || !photo) return;
    const ui = useUIStore.getState();
    const before = ui.customEscapeHandler;
    const handler = () => useProviderLooks.getState().cancel(photo);
    ui.setCustomEscapeHandler(handler);
    return () => {
      const now = useUIStore.getState();
      if (now.customEscapeHandler === handler) now.setCustomEscapeHandler(before);
    };
  }, [spinning, photo]);

  if (!offer?.photo || !subject) return null;

  const { on } = offer;
  const own = offer.photo.camera;
  const camera = cameraOf(subject, adjustments) ?? own;
  const onPhoto = photo ? lookOf(adjustments) : null;
  const fileOf = (look: OfferedLook) => files.find((f) => f.camera === camera && f.look === look.id);
  const isOn = (look: OfferedLook) => onPhoto?.camera === camera && onPhoto.look === look.id;

  const stopPreview = () => {
    window.clearTimeout(hover.current);
    setEditor({ previewOverride: null });
  };

  /** Hover never starts a job: a look with no file has nothing to show. */
  const startPreview = (look: OfferedLook) => {
    if (!photo || !on || !fileOf(look)) return;
    window.clearTimeout(hover.current);
    hover.current = window.setTimeout(() => {
      const now = useEditorStore.getState().adjustments;
      setEditor({ previewOverride: { ...now, ...lookOnto(now, lookKeys(camera, look.id, look.name)) } });
    }, PREVIEW_AFTER_MS);
  };

  const choose = (look: OfferedLook) => {
    stopPreview();
    if (!photo) {
      const { multiSelectedPaths } = useLibraryStore.getState();
      const paths = multiSelectedPaths.length > 0 ? multiSelectedPaths : [subject];
      const chosen = useProviderLooks.getState().chosen[subject] ?? null;
      putOnSelection(paths, look, chosen);
    } else if (isOn(look)) onTakeOff();
    else pick(photo, look, camera);
  };

  return (
    <div className="agm-provider" onMouseLeave={stopPreview}>
      <div className="agm-provider-head">
        <span className="min-w-0 truncate">{t('looksProvider')}</span>
        <button
          aria-checked={on}
          aria-label={t('looksProvider')}
          className="agm-switch"
          onClick={() => setOn(!on)}
          role="switch"
        >
          <span>{t(on ? 'looksProviderOn' : 'looksProviderOff')}</span>
          <i />
        </button>
      </div>
      <div className="agm-provider-note">{t(on ? 'looksProviderOnNote' : 'looksProviderOffNote')}</div>
      <div className={clsx('agm-provider-camera', !on && 'agm-look-dim')}>
        <label htmlFor="agm-provider-camera">{t('looksProviderCamera')}</label>
        <select
          disabled={!on}
          id="agm-provider-camera"
          onChange={(e) => chooseCamera(subject, e.target.value, adjustments)}
          value={camera}
        >
          {offer.cameras.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
        <span className={clsx('agm-provider-matched', camera !== own && 'agm-provider-other')}>
          {t(camera === own ? 'looksProviderMatched' : 'looksProviderOther')}
        </span>
      </div>
      {offer.looks.map((look) => {
        const file = fileOf(look);
        const swatch = file && swatches[file.path];
        return (
          <div key={look.id}>
            <div
              className={clsx('agm-look', isOn(look) && 'agm-on', !on && 'agm-look-dim')}
              onClick={() => choose(look)}
              onMouseEnter={() => startPreview(look)}
            >
              <span className="agm-swatch" style={swatch ? { backgroundImage: `url(${swatch})` } : undefined} />
              <span className="agm-look-name">{look.name}</span>
              {subject && waiting[subject]?.look === look.id && waiting[subject].spinning && (
                <span aria-label={t('looksProviderMaking')} className="agm-spin" />
              )}
            </div>
            {note?.photo === subject && note.look === look.id && (
              <div className="agm-provider-rownote">{t(note.text)}</div>
            )}
          </div>
        );
      })}
      {batch && (
        <div className="agm-provider-batch">
          <span>
            {t('looksProviderBatch').replace('{done}', String(batch.done)).replace('{total}', String(batch.total))}
          </span>
          <button onClick={cancelBatch}>{t('looksProviderCancel')}</button>
        </div>
      )}
    </div>
  );
}

interface StripProps {
  /** The provider can make its file. */
  canMake: boolean;
  /** Its camera. */
  kind: string;
  name: string;
  onMake: () => void;
  onStrength: (value: number) => void;
  onTakeOff: () => void;
  strength: number;
  swatch: string | undefined;
  /** Its file is beside the photo. */
  there: boolean;
}

/** A provider look on the photo: as a LUT's row, with what only it has. */
export function LookStrip(props: StripProps) {
  const t = useAgTranslation();
  const { canMake, kind, name, onMake, onStrength, onTakeOff, strength, swatch, there } = props;
  return (
    <div className="agm-onphoto-item">
      <div className="agm-look agm-on">
        <span className="agm-swatch" style={swatch ? { backgroundImage: `url(${swatch})` } : undefined} />
        <span className="agm-look-name">{name}</span>
        <span className="agm-kind">{kind}</span>
        <button className="agm-btn" onClick={onTakeOff} data-tooltip={t('looksTakeOff')}>
          <X size={12} />
        </button>
      </div>
      {there ? (
        <Slider
          label={t('looksStrength')}
          min={0}
          max={200}
          step={1}
          defaultValue={100}
          value={strength}
          onChange={(e) => onStrength(Number(e.target.value))}
        />
      ) : (
        <div className="agm-provider-missing">{t('looksProviderMissing')}</div>
      )}
      {canMake && (
        <div className="agm-provider-tools">
          <button onClick={onMake}>{t(there ? 'looksProviderRemake' : 'looksProviderMakeAgain')}</button>
        </div>
      )}
    </div>
  );
}

/** The look on the open photo, read from its edit and its file. */
export function OnPhotoLook({
  photo,
  onStrength,
  onTakeOff,
}: {
  photo: string;
  onStrength: (value: number) => void;
  onTakeOff: () => void;
}) {
  const t = useAgTranslation();
  const adjustments = useEditorStore((s) => s.adjustments);
  const { offer, filesMade, keepLook } = useProviderLooks();
  const on = lookOf(adjustments);
  const camera = on?.camera ?? '';
  const look = on?.look ?? '';
  const [there, setThere] = useState(true);

  useEffect(() => {
    let current = true;
    ag('provider_look_file', { path: photo, camera, look })
      .then((found) => current && setThere(found))
      .catch(() => current && setThere(false));
    return () => {
      current = false;
    };
  }, [photo, camera, look, filesMade]);

  const file = offer?.photo?.files.find((f) => f.camera === camera && f.look === look);
  const swatches = useFileSwatches(file ? [file] : [], photo, true);
  const offered = offer?.looks.find((l) => l.id === look);
  const listed = !!offered && !!offer?.cameras.some((c) => c.id === camera);

  return (
    <LookStrip
      canMake={listed}
      kind={offer?.cameras.find((c) => c.id === camera)?.name ?? t('looksProviderKind')}
      name={adjustments.lutName ?? ''}
      onMake={() => offered && keepLook([photo], offered, camera, true)}
      onStrength={onStrength}
      onTakeOff={onTakeOff}
      strength={adjustments.lutIntensity ?? 100}
      swatch={file && swatches[file.path]}
      there={there}
    />
  );
}
