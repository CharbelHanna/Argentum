/**
 * The RAW decoder: the app's, in Settings > Processing, and a photo's own,
 * first in Edit > Profile. Ours.
 *
 * rawler is today's decoder; FlashRaw unpacks the pixels faster, inside
 * rawler's reading of the file, on the formats where it was measured to give
 * the same image (mods/raw_decoder.rs, the engine's flashraw.rs). So the
 * picture is the same either way, and a file FlashRaw does not handle opens
 * with rawler, which the photo's row says.
 *
 * A photo's choice is `rawDecoder` in its edit, none meaning the app's. It is
 * the file's: a virtual copy shares its original's decode, so it shows no
 * row. A change goes three ways: into the edit, where their save persists it
 * (the sidecar's one writer, see CameraProfile.tsx), to the decoder for this
 * session, and a decode of the open photo.
 */

import { useEffect, useState } from 'react';
import Dropdown from '../components/ui/Dropdown';
import { useEditorActions } from '../hooks/useEditorActions';
import { useEditorStore } from '../store/useEditorStore';
import type { Adjustments } from '../utils/adjustments';
import { ag } from './ag';
import { useAgTranslation } from './locales';
import { redecodeOpenPhoto } from './redecode';
import { isVirtualCopyPath } from './paths';

/** `RawDecoder` in src-tauri/src/mods/raw_decoder.rs. */
type RawDecoderName = 'rawler' | 'flashraw';

const NAMES: Record<RawDecoderName, string> = { rawler: 'rawler', flashraw: 'FlashRaw' };

/** The entry that means "the app's". */
const DEFAULT = '';

const reportRedecode = (e: unknown) => console.warn('Re-decoding the open photo failed:', e);

/** Settings > Processing: the app's decoder. */
export function RawDecoderSetting() {
  const t = useAgTranslation();
  const [value, setValue] = useState<RawDecoderName | null>(null);

  useEffect(() => {
    ag<RawDecoderName>('raw_decoder')
      .then(setValue)
      .catch(() => setValue(null));
  }, []);

  if (!value) {
    return null;
  }

  const change = async (next: RawDecoderName) => {
    const before = value;
    setValue(next);
    try {
      await ag('set_raw_decoder', { decoder: next });
    } catch {
      setValue(before);
      return;
    }
    redecodeOpenPhoto().catch(reportRedecode);
  };

  return (
    <Dropdown
      onChange={change}
      options={[
        { value: 'rawler', label: t('rawDecoderRawler') },
        { value: 'flashraw', label: t('rawDecoderFlashRaw') },
      ]}
      value={value}
      triggerClassName="bg-bg-primary"
    />
  );
}

/** Edit > Profile: this photo's decoder. */
export default function RawDecoder() {
  const t = useAgTranslation();
  const path = useEditorStore((s) => s.selectedImage?.path);
  const isRaw = useEditorStore((s) => s.selectedImage?.isRaw === true);
  const choice = useEditorStore((s) => (s.adjustments as { rawDecoder?: RawDecoderName }).rawDecoder);
  const { setAdjustments } = useEditorActions();
  const [appDecoder, setAppDecoder] = useState<RawDecoderName | null>(null);
  const [handled, setHandled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const shown = !!path && isRaw && !isVirtualCopyPath(path);

  useEffect(() => {
    if (!shown) {
      return;
    }
    let live = true;
    ag<RawDecoderName>('raw_decoder')
      .then((d) => live && setAppDecoder(d))
      .catch(() => live && setAppDecoder(null));
    return () => {
      live = false;
    };
  }, [path, shown]);

  const used = choice ?? appDecoder;

  // Asked only when FlashRaw is chosen: answering reads the file, which a
  // photo on rawler should not pay for.
  useEffect(() => {
    setHandled(null);
    if (!shown || used !== 'flashraw') {
      return;
    }
    let live = true;
    ag<boolean>('flashraw_handles', { path })
      .then((h) => live && setHandled(h))
      .catch(() => live && setHandled(null));
    return () => {
      live = false;
    };
  }, [path, shown, used]);

  if (!shown || !appDecoder) {
    return null;
  }

  const choose = async (value: string) => {
    const decoder = value === DEFAULT ? undefined : (value as RawDecoderName);
    setBusy(true);
    setAdjustments((prev) => ({ ...prev, rawDecoder: decoder }) as Adjustments);
    try {
      await ag('set_photo_raw_decoder', { path, decoder: decoder ?? null });
      await redecodeOpenPhoto();
    } catch (e) {
      reportRedecode(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <div className="flex items-center gap-3">
        <span className="shrink-0 text-sm font-medium text-text-secondary select-none">{t('rawDecoderLabel')}</span>
        <select
          value={choice ?? DEFAULT}
          disabled={busy}
          onChange={(e) => choose(e.target.value)}
          className="min-w-0 flex-1 text-xs bg-bg-primary text-text-primary rounded px-2 py-1.5 truncate disabled:opacity-50"
        >
          <option value={DEFAULT}>{`${t('rawDecoderDefault')} (${NAMES[appDecoder]})`}</option>
          <option value="rawler">{NAMES.rawler}</option>
          <option value="flashraw">{NAMES.flashraw}</option>
        </select>
      </div>
      {used === 'flashraw' && handled === false && (
        <p className="mt-1 text-xs text-text-secondary">{t('rawDecoderRawlerOnly')}</p>
      )}
    </div>
  );
}
