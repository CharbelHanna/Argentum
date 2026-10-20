/**
 * Edit > Effects > Relight. Ours, from RapidRAW's.
 *
 * Their Relight (Effects.tsx on main at 454789b7: ca5feaa3, 68b14a36, 653f1dcd)
 * sits inside their "Spatial" part with Lens blur and Fog. Here it is a part of
 * its own after Lens blur, laid out as the 2026-10-20 placement note has it:
 * the switch, then what applies to every light (so it does not jump when the
 * list or a type changes), the lights, then the selected light by its type.
 * Their sliders, ranges and defaults are kept; the words are ours where
 * renames.ts says so.
 *
 * The lights' handles on the photo are in onPhoto.tsx; this part shows and
 * hides them through the editor store, as their panel does.
 */

import { useCallback, useState, type MouseEvent } from 'react';
import { CopyPlus, Plus, X } from 'lucide-react';
import clsx from 'clsx';
import Slider, { type SliderChangeEvent } from '../../components/ui/Slider';
import Switch from '../../components/ui/Switch';
import Text from '../../components/ui/Text';
import ColorField, { normalizeHexColor, useParsedTextField } from '../../components/ui/ColorField';
import { TextVariants } from '../../types/typography';
import { useEditorStore } from '../../store/useEditorStore';
import { Adjustments, Effect, RelightLight, createRelightLight, getRelightLightColor } from '../../utils/adjustments';
import { useAgTranslation, type AgKey } from '../locales';
import { RELIGHT_MAP, useEffectMap } from './effectMap';
import { addLight, duplicateLight, hideLights, removeLight, showLight, useLightMenu } from './lights';
import MapStatus from './MapStatus';
import { useTheirText } from './theirText';

type SetAdjustments = (update: (prev: Adjustments) => Adjustments) => void;

interface RelightPartProps {
  adjustments: Adjustments;
  setAdjustments: SetAdjustments;
  onDragStateChange?: (isDragging: boolean) => void;
}

const TYPES: { type: RelightLight['type']; label: string; tip: AgKey }[] = [
  { type: 'point', label: 'adjustments.effects.relightTypes.point', tip: 'fx166LampTip' },
  { type: 'spot', label: 'adjustments.effects.relightTypes.spot', tip: 'fx166SpotTip' },
  { type: 'directional', label: 'adjustments.effects.relightTypes.directional', tip: 'fx166SunTip' },
];

/** A label between groups of sliders, as Basic's "Tone" and "Presence". */
const SubHeading = ({ children }: { children: string }) => (
  <div className="pt-2 pb-1 text-center text-xs text-text-secondary select-none">{children}</div>
);

export default function RelightPart({ adjustments, setAdjustments, onDragStateChange }: RelightPartProps) {
  const t = useTheirText();
  const ag = useAgTranslation();
  const [hovered, setHovered] = useState<string | null>(null);
  const isShown = useEditorStore((s) => s.isRelightPickerActive);
  const activeId = useEditorStore((s) => s.activeRelightLightId);
  const openLightMenu = useLightMenu(setAdjustments);

  const lights: RelightLight[] = adjustments.relightLights || [];
  const enabled = !!adjustments.relightEnabled;
  const hasMap = !!adjustments.relightNormalMap;
  // The settings below are the selected light's; with none selected yet, the first's.
  const selectedIndex = Math.max(
    0,
    lights.findIndex((l) => l.id === activeId),
  );
  const selected: RelightLight | undefined = lights[selectedIndex];

  const set = (key: string, value: number | boolean) => setAdjustments((prev) => ({ ...prev, [key]: value }));
  const fromSlider = (e: SliderChangeEvent) => parseInt(String(e.target.value), 10);

  /** On, with Light 1 in the middle when there is none yet, and the lights showing. */
  const turnOn = useCallback(
    (map?: string) => {
      const lamp = createRelightLight(0.5, 0.5);
      setAdjustments((prev) => ({
        ...prev,
        relightEnabled: true,
        relightNormalMap: map ?? prev.relightNormalMap,
        relightLights: prev.relightLights?.length ? prev.relightLights : [lamp],
      }));
      showLight(useEditorStore.getState().adjustments.relightLights?.[0]?.id ?? lamp.id);
    },
    [setAdjustments],
  );

  const turnOff = useCallback(() => {
    setAdjustments((prev) => (prev.relightEnabled ? { ...prev, relightEnabled: false } : prev));
    hideLights();
  }, [setAdjustments]);

  const map = useEffectMap(RELIGHT_MAP, { enabled, hasMap, turnOn, turnOff });

  const selectRow = (id: string) => {
    if (id === activeId && isShown) hideLights();
    else showLight(id);
  };

  const onRowMenu = (e: MouseEvent, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    openLightMenu(e.clientX, e.clientY, id);
  };

  const changeLight = (key: keyof RelightLight, value: number | string) => {
    if (!selected) return;
    const id = selected.id;
    setAdjustments((prev) => ({
      ...prev,
      relightLights: (prev.relightLights || []).map((l: RelightLight) => (l.id === id ? { ...l, [key]: value } : l)),
    }));
  };
  const lightSlider = (key: keyof RelightLight) => (e: SliderChangeEvent) => changeLight(key, fromSlider(e));

  const colorField = useParsedTextField(
    selected?.color ?? '#ffffff',
    (c) => changeLight('color', c),
    normalizeHexColor,
  );

  const ready = enabled && hasMap && map.phase.kind === 'idle';

  return (
    <div data-relight-lights>
      <Switch label={t('adjustments.effects.relight')} checked={map.isOn} onChange={map.toggle} />

      <MapStatus
        phase={map.phase}
        ask="fx166RelightAsk"
        downloading="fx166RelightDownloading"
        working={t('editor.ai.generatingNormalMap')}
        onDownload={map.download}
        onNotNow={map.notNow}
      />

      <div
        className={`grid transition-all duration-300 ease-in-out ${
          ready ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'
        }`}
      >
        <div className="overflow-hidden">
          <div className="pt-2 pb-1">
            <SubHeading>{ag('fx166AllLights')}</SubHeading>
            <Slider
              label={t('adjustments.effects.relightAmbient')}
              max={100}
              min={-100}
              onChange={(e) => set(Effect.RelightAmbient, fromSlider(e))}
              step={1}
              value={adjustments.relightAmbient ?? 0}
              onDragStateChange={onDragStateChange}
            />
            <Slider
              label={t('adjustments.effects.relightSoftness')}
              max={100}
              min={0}
              defaultValue={25}
              onChange={(e) => set(Effect.RelightSoftness, fromSlider(e))}
              step={1}
              value={adjustments.relightSoftness ?? 25}
              onDragStateChange={onDragStateChange}
              fillOrigin="min"
            />
            <Slider
              label={t('adjustments.effects.relightShine')}
              max={100}
              min={0}
              onChange={(e) => set(Effect.RelightShine, fromSlider(e))}
              step={1}
              value={adjustments.relightShine ?? 0}
              onDragStateChange={onDragStateChange}
            />
            <Switch
              className="py-1"
              label={t('adjustments.effects.relightShadows')}
              checked={!!adjustments.relightShadows}
              onChange={(on: boolean) => set(Effect.RelightShadows, on)}
            />
            {adjustments.relightShadows && (
              <Slider
                label={t('adjustments.effects.relightShadowSoftness')}
                max={100}
                min={0}
                defaultValue={15}
                onChange={(e) => set(Effect.RelightShadowSoftness, fromSlider(e))}
                step={1}
                value={adjustments.relightShadowSoftness ?? 15}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
            )}

            <SubHeading>{ag('fx166Lights')}</SubHeading>
            <div className="flex flex-col gap-0.5">
              {lights.map((light, index) => (
                <div
                  key={light.id}
                  className={clsx(
                    'group flex items-center gap-2 px-2 py-1 rounded-md cursor-pointer select-none text-sm',
                    light.id === selected?.id
                      ? isShown
                        ? 'bg-card-active text-text-primary'
                        : 'bg-surface text-text-primary'
                      : 'text-text-secondary hover:bg-surface',
                  )}
                  onClick={() => selectRow(light.id)}
                  onContextMenu={(e) => onRowMenu(e, light.id)}
                  onMouseEnter={() => setHovered(light.id)}
                  onMouseLeave={() => setHovered(null)}
                >
                  <span
                    className="w-3 h-3 rounded-full shrink-0 ring-1 ring-black/30"
                    style={{ backgroundColor: getRelightLightColor(light) }}
                  />
                  <span>{t('adjustments.effects.relightLight', { index: index + 1 })}</span>
                  <span className="text-xs text-text-secondary">
                    {t(TYPES.find((x) => x.type === light.type)?.label ?? '')}
                  </span>
                  <span className={clsx('ml-auto flex gap-1', hovered === light.id ? 'opacity-100' : 'opacity-0')}>
                    <button
                      className="p-0.5 rounded text-text-secondary hover:text-text-primary"
                      data-tooltip={t('adjustments.effects.relightDuplicateLight')}
                      aria-label={t('adjustments.effects.relightDuplicateLight')}
                      onClick={(e) => {
                        e.stopPropagation();
                        duplicateLight(setAdjustments, light.id);
                      }}
                    >
                      <CopyPlus size={13} />
                    </button>
                    <button
                      className="p-0.5 rounded text-text-secondary hover:text-red-500"
                      data-tooltip={t('adjustments.effects.relightRemoveLight')}
                      aria-label={t('adjustments.effects.relightRemoveLight')}
                      onClick={(e) => {
                        e.stopPropagation();
                        removeLight(setAdjustments, light.id);
                      }}
                    >
                      <X size={13} />
                    </button>
                  </span>
                </div>
              ))}
              <button
                className="flex items-center gap-2 px-2 py-1 rounded-md text-sm text-text-secondary hover:text-text-primary hover:bg-surface"
                onClick={() => addLight(setAdjustments)}
              >
                <Plus size={14} />
                {t('adjustments.effects.relightAddLight')}
              </button>
            </div>
            {lights.length > 0 && (
              <Text variant={TextVariants.small} className="block px-2 pt-1 text-text-secondary">
                {ag(isShown ? 'fx166LightsHint' : 'fx166LightsHidden')}
              </Text>
            )}

            {selected && (
              <>
                <SubHeading>{t('adjustments.effects.relightLight', { index: selectedIndex + 1 })}</SubHeading>
                <div className="flex items-center justify-between gap-2 pb-2">
                  <Text variant={TextVariants.label} className="text-text-secondary">
                    {t('adjustments.effects.relightType')}
                  </Text>
                  <div className="flex p-0.5 bg-bg-primary rounded-md">
                    {TYPES.map(({ type, label, tip }) => (
                      <button
                        key={type}
                        className={clsx(
                          'px-3 py-1 text-sm rounded-md transition-colors',
                          selected.type === type
                            ? 'bg-accent text-button-text'
                            : 'text-text-secondary hover:text-text-primary hover:bg-surface',
                        )}
                        data-tooltip={ag(tip)}
                        onClick={() => {
                          changeLight('type', type);
                          showLight(selected.id);
                        }}
                      >
                        {t(label)}
                      </button>
                    ))}
                  </div>
                </div>
                <ColorField
                  color={selected.color || '#ffffff'}
                  disabled={false}
                  field={colorField}
                  label={t('adjustments.effects.relightColor')}
                  onColorChange={(c) => changeLight('color', c)}
                />
                <Slider
                  label={t('adjustments.effects.relightIntensity')}
                  max={100}
                  min={0}
                  defaultValue={60}
                  onChange={lightSlider('intensity')}
                  step={1}
                  value={selected.intensity}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                {selected.type !== 'point' && (
                  <>
                    <Slider
                      label={t('adjustments.effects.relightAngle')}
                      max={360}
                      min={0}
                      defaultValue={135}
                      onChange={lightSlider('angle')}
                      step={1}
                      suffix="°"
                      value={selected.angle}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      label={t('adjustments.effects.relightElevation')}
                      max={180}
                      min={-180}
                      defaultValue={60}
                      onChange={lightSlider('elevation')}
                      step={1}
                      suffix="°"
                      value={selected.elevation}
                      onDragStateChange={onDragStateChange}
                    />
                  </>
                )}
                {selected.type === 'spot' && (
                  <>
                    <Slider
                      label={t('adjustments.effects.relightCone')}
                      max={100}
                      min={0}
                      defaultValue={40}
                      onChange={lightSlider('cone')}
                      step={1}
                      value={selected.cone}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      label={t('adjustments.effects.feather')}
                      max={100}
                      min={0}
                      defaultValue={50}
                      onChange={lightSlider('feather')}
                      step={1}
                      value={selected.feather}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                  </>
                )}
                {selected.type !== 'directional' && (
                  <>
                    <Slider
                      label={t('adjustments.effects.relightDepth')}
                      max={100}
                      min={0}
                      defaultValue={0}
                      onChange={lightSlider('depth')}
                      step={1}
                      value={selected.depth}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      label={t('adjustments.effects.relightFalloff')}
                      max={100}
                      min={0}
                      defaultValue={30}
                      onChange={lightSlider('radius')}
                      step={1}
                      value={selected.radius}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                  </>
                )}
                <Slider
                  label={t('adjustments.color.temperature')}
                  max={100}
                  min={-100}
                  onChange={lightSlider('temperature')}
                  step={1}
                  value={selected.temperature}
                  trackClassName="temperature-gradient-track"
                  onDragStateChange={onDragStateChange}
                />
                <Slider
                  label={t('adjustments.color.tint')}
                  max={100}
                  min={-100}
                  onChange={lightSlider('tint')}
                  step={1}
                  value={selected.tint}
                  trackClassName="tint-gradient-track"
                  onDragStateChange={onDragStateChange}
                />
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
