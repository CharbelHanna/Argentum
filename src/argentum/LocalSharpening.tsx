/**
 * A mask's own Sharpen, in the Details of the Masks panel. Ours.
 *
 * Mounted into the same marker as the main Sharpening card, in the copy of
 * their Details panel the Masks panel renders for the active mask (flagged
 * `data-mask`). Their local Sharpness slider beside it is hidden by
 * `sharpening.css`, as the global one is.
 *
 * One slider, as Lightroom's local Sharpness: right sharpens with darktable's
 * unsharp mask, left softens towards its blur. Under More, that unsharp
 * mask's own Radius, Mask and Threshold - each mask has its own, and nothing
 * in the main card reaches them. The eye shows where it lands: inside the
 * mask, where its Mask lets it. It runs on our engine (`mods/sharpen.rs`,
 * `locals`) and is stored in the mask's own adjustments as `agSharpen`,
 * written the way their panel writes the mask: `adjustments.masks[]` by
 * `activeMaskContainerId`.
 */

import { useEffect } from 'react';
import Slider from '../components/ui/Slider';
import Text from '../components/ui/Text';
import { TextVariants } from '../types/typography';
import { useEditorStore } from '../store/useEditorStore';
import { useEditorActions } from '../hooks/useEditorActions';
import { useAgTranslation } from './locales';
import { Chip, EyeButton, More, useAutoValues } from './Sharpening';
import { AgLocalSharpen, LOCAL_MASK, LOCAL_SHARPEN_DEFAULTS, useSharpenMask } from './sharpenSettings';

export default function LocalSharpening() {
  const t = useAgTranslation();
  const adjustments = useEditorStore((s: any) => s.adjustments);
  const activeId: string | null = useEditorStore((s: any) => s.activeMaskContainerId);
  const path: string | undefined = useEditorStore((s: any) => s.selectedImage?.path);
  const setEditor = useEditorStore((s: any) => s.setEditor);
  const { setAdjustments } = useEditorActions();
  const auto = useAutoValues(path, adjustments);
  const view = useSharpenMask();

  // Another mask chosen: the eye was on this one's.
  const { clear } = view;
  useEffect(() => clear(), [activeId, clear]);

  const mask = adjustments?.masks?.find((m: any) => m.id === activeId);
  if (!mask) {
    return null;
  }
  const s: AgLocalSharpen = { ...LOCAL_SHARPEN_DEFAULTS, ...(mask.adjustments?.agSharpen ?? {}) };
  const legacy = Number(mask.adjustments?.sharpness) || 0;
  const contrastShown = s.autoContrast ? (auto.contrast ?? s.contrast) : s.contrast;

  const update = (patch: (own: any) => Record<string, unknown>) =>
    setAdjustments((prev: any) => ({
      ...prev,
      masks: prev.masks.map((m: any) =>
        m.id === activeId ? { ...m, adjustments: { ...m.adjustments, ...patch(m.adjustments ?? {}) } } : m,
      ),
    }));
  const set = (patch: Partial<AgLocalSharpen>) =>
    update((own) => ({ agSharpen: { ...(own.agSharpen ?? {}), ...patch } }));
  const num = (e: any) => parseFloat(String(e.target.value));
  const held = view.dragOf(LOCAL_MASK);
  const onDrag = (dragging: boolean) => {
    setEditor({ isSliderDragging: dragging });
    held(dragging);
  };

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-end gap-1">
        <div className="grow">
          <Slider
            label={t('sharpenLocal')}
            min={-100}
            max={100}
            step={1}
            value={s.amount}
            defaultValue={0}
            onChange={(e: any) => set({ amount: num(e) })}
            onDragStateChange={onDrag}
          />
        </div>
        <EyeButton
          on={view.pinned === LOCAL_MASK}
          title={t('sharpenShowMaskHelp')}
          onClick={() => view.toggle(LOCAL_MASK)}
        />
      </div>
      <More>
        <Slider
          label={t('sharpenRadius')}
          min={0.1}
          max={8}
          step={0.05}
          value={s.radius}
          defaultValue={LOCAL_SHARPEN_DEFAULTS.radius}
          fillOrigin="min"
          onChange={(e: any) => set({ radius: num(e) })}
          onDragStateChange={onDrag}
        />
        {/* Softening goes towards the blur everywhere in the mask: no mask or threshold to set. */}
        {s.amount >= 0 && (
          <>
            <div className="flex items-end gap-1">
              <div className="grow">
                <Slider
                  label={t('sharpenMask')}
                  min={0}
                  max={200}
                  step={1}
                  value={Math.round(contrastShown)}
                  defaultValue={LOCAL_SHARPEN_DEFAULTS.contrast}
                  fillOrigin="min"
                  onChange={(e: any) => set({ autoContrast: false, contrast: num(e) })}
                  onDragStateChange={onDrag}
                />
              </div>
              <Chip
                on={s.autoContrast}
                label={t('sharpenAuto')}
                title={t('sharpenAutoContrastHelp')}
                onClick={() => set({ autoContrast: !s.autoContrast, contrast: Math.round(contrastShown) })}
              />
            </div>
            <Slider
              label={t('sharpenThreshold')}
              min={0}
              max={10}
              step={0.1}
              value={s.threshold}
              defaultValue={LOCAL_SHARPEN_DEFAULTS.threshold}
              fillOrigin="min"
              onChange={(e: any) => set({ threshold: num(e) })}
              onDragStateChange={onDrag}
            />
          </>
        )}
      </More>
      <Text variant={TextVariants.small} className="text-text-secondary">
        {t('sharpenLocalHint')}
      </Text>
      {legacy !== 0 && (
        <div className="flex items-center justify-between gap-2 rounded bg-bg-primary px-2 py-1">
          <Text variant={TextVariants.small} className="text-text-secondary">
            {t('sharpenLegacy').replace('{n}', String(legacy))}
          </Text>
          <button
            type="button"
            className="shrink-0 text-[11px] text-accent hover:underline"
            onClick={() => update(() => ({ sharpness: 0 }))}
          >
            {t('sharpenLegacyRemove')}
          </button>
        </div>
      )}
    </div>
  );
}
