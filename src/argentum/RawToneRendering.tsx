/**
 * The RAW tone rendering choice. Ours.
 *
 * This is deliberately separate from RapidRAW's AgX/basic tone-mapper switch.
 * It answers a different question: which camera-style rendering should happen
 * at the RAW display-transform stage? Default is the existing Argentum path;
 * Base Curve is a camera-style curve; Auto-Matched is fitted from the JPEG
 * embedded in this RAW file; DNG Standard is a DNG renderer's default - the
 * camera's exposure offset and Adobe's default tone curve, applied as Adobe
 * applies it (mods/dng_standard.rs); Linear is DNG Standard without the tone
 * curve, the neutral base a film-simulation LUT is made for.
 *
 * The curve is fitted to this photo, so Copy settings and presets carry the
 * rendering but never `rawToneCurve` (mods/copy_keys.rs). `rawToneCurveMode`
 * names the rendering a curve was fitted for: a photo that receives another
 * rendering fits its own here, and until then the renderer ignores the old
 * curve (mods/raw_tone.rs `points_of`).
 */

import { useCallback, useEffect, useState } from 'react';
import { useEditorStore } from '../store/useEditorStore';
import { useEditorActions } from '../hooks/useEditorActions';
import { ag } from './ag';
import { rawToneRenderingOf } from './editSections';
import { useAgTranslation } from './locales';
import type { HeldBack } from './ag.generated';

type RawToneRenderingMode = 'default' | 'baseCurve' | 'autoMatched' | 'dngStandard' | 'linear';

const MODES: RawToneRenderingMode[] = ['default', 'baseCurve', 'autoMatched', 'dngStandard', 'linear'];

/** The renderings the renderer fills in from the photo itself: nothing to fit here. */
const READ_BY_RENDERER: RawToneRenderingMode[] = ['dngStandard', 'linear'];

const HELD_BACK_NAMES = {
  fujifilmDr200: 'heldBackFujifilmDr200',
  fujifilmDr400: 'heldBackFujifilmDr400',
  canonHighlightTonePriority: 'heldBackCanonHighlightTonePriority',
  nikonHlg: 'heldBackNikonHlg',
  nikonActiveDLighting: 'heldBackNikonActiveDLighting',
  olympusGradation: 'heldBackOlympusGradation',
  pentaxDynamicRange: 'heldBackPentaxDynamicRange',
} as const;

/**
 * Under DNG Standard, the note that says a camera mode recorded this photo
 * darker and how much DNG Standard gives back, so a brighter start has a reason
 * where the rendering is chosen.
 */
function HeldBackNote({ path }: { path: string }) {
  const t = useAgTranslation();
  const [heldBack, setHeldBack] = useState<HeldBack | null>(null);
  useEffect(() => {
    let current = true;
    setHeldBack(null);
    ag('dng_standard_held_back', { path })
      .then((found) => current && setHeldBack(found))
      .catch((error) => console.warn('Could not read what the camera held back', error));
    return () => {
      current = false;
    };
  }, [path]);
  if (!heldBack) return null;
  const ev = heldBack.ev.toFixed(1);
  const fill = (text: string) => text.replace('{mode}', t(HELD_BACK_NAMES[heldBack.mode])).replaceAll('{ev}', ev);
  return (
    <p className="mt-1 text-xs text-text-secondary" data-tooltip={fill(t('heldBackTip'))}>
      ⓘ {fill(t('heldBackNote'))}
    </p>
  );
}

export default function RawToneRendering() {
  const t = useAgTranslation();
  const selectedImage = useEditorStore((s: any) => s.selectedImage);
  const adjustments = useEditorStore((s: any) => s.adjustments);
  const { setAdjustments } = useEditorActions();
  const path: string | undefined = selectedImage?.path;
  const isRaw = selectedImage?.isRaw === true;
  const asked = rawToneRenderingOf(adjustments);
  const current: RawToneRenderingMode = MODES.includes(asked as RawToneRenderingMode)
    ? (asked as RawToneRenderingMode)
    : 'default';
  const [busy, setBusy] = useState(false);

  const ensureCurve = useCallback(
    async (mode: Exclude<RawToneRenderingMode, 'default'>) => {
      if (!path) return;
      setBusy(true);
      try {
        const curve = await ag('raw_tone_curve', { path, mode });
        if (useEditorStore.getState().selectedImage?.path !== path) return;
        setAdjustments((prev: any) => ({
          ...prev,
          rawToneRendering: mode,
          rawToneCurve: curve,
          rawToneCurveMode: mode,
        }));
      } catch (error) {
        console.warn(`Could not calculate RAW tone curve (${mode})`, error);
      } finally {
        setBusy(false);
      }
    },
    [path, setAdjustments],
  );

  // DNG Standard and Linear need nothing fitted: the renderer reads the photo's
  // curve itself (raw_tone::fill_missing), so a RAW that merely opens in it is not
  // written to. A curve fitted for another rendering - left behind when a
  // rendering is pasted onto the photo - counts as none.
  const fitted = adjustments?.rawToneCurve?.length >= 2 && (adjustments?.rawToneCurveMode ?? current) === current;
  useEffect(() => {
    if (!isRaw || !path || current === 'default' || READ_BY_RENDERER.includes(current) || fitted || busy) {
      return;
    }
    ensureCurve(current);
  }, [fitted, busy, current, ensureCurve, isRaw, path]);

  if (!isRaw) return null;

  const choose = async (mode: RawToneRenderingMode) => {
    if (busy || mode === current) return;
    if (mode === 'default') {
      setAdjustments((prev: any) => ({
        ...prev,
        rawToneRendering: 'default',
        rawToneCurve: null,
        rawToneCurveMode: null,
      }));
      return;
    }
    await ensureCurve(mode);
  };

  return (
    <div>
      <div className="flex items-center gap-3">
        <span className="shrink-0 text-sm font-medium text-text-secondary select-none">{t('rawToneLabel')}</span>
        <select
          value={current}
          disabled={busy}
          onChange={(e) => choose(e.target.value as RawToneRenderingMode)}
          className="min-w-0 flex-1 text-xs bg-bg-primary text-text-primary rounded px-2 py-1.5 truncate disabled:opacity-50"
          data-tooltip={t('rawToneHelp')}
        >
          <option value="default">{t('rawToneDefault')}</option>
          <option value="baseCurve">{t('rawToneBaseCurve')}</option>
          <option value="autoMatched">{t('rawToneAutoMatched')}</option>
          <option value="dngStandard">{t('rawToneDngStandard')}</option>
          <option value="linear">{t('rawToneLinear')}</option>
        </select>
      </div>
      {busy && <p className="mt-1 text-xs text-text-secondary">{t('rawToneCalculating')}</p>}
      {READ_BY_RENDERER.includes(current) && path && <HeldBackNote path={path} />}
    </div>
  );
}
