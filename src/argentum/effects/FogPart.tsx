/**
 * Edit > Effects > Fog. Ours, from RapidRAW's.
 *
 * Their Fog (Effects.tsx on main at 454789b7, from 1cd88e76) as a part of its
 * own after Relight instead of inside their "Spatial" part: the switch, then
 * their sliders with their ranges and defaults. The depth map is the one Lens
 * blur and the Depth mask use; its model is asked for only when missing.
 */

import { useCallback } from 'react';
import Slider, { type SliderChangeEvent } from '../../components/ui/Slider';
import Switch from '../../components/ui/Switch';
import { Adjustments, Effect } from '../../utils/adjustments';
import { DEPTH_MAP, useEffectMap } from './effectMap';
import MapStatus from './MapStatus';
import { useTheirText } from './theirText';

interface FogPartProps {
  adjustments: Adjustments;
  setAdjustments(update: (prev: Adjustments) => Adjustments): void;
  onDragStateChange?: (isDragging: boolean) => void;
}

/** Their sliders, in their order: key, label, default, and whether they run from -100. */
const SLIDERS: { key: Effect; label: string; initial: number; signed?: boolean; track?: string }[] = [
  { key: Effect.FogAmount, label: 'adjustments.effects.amount', initial: 50 },
  { key: Effect.FogStart, label: 'adjustments.effects.fogStart', initial: 0 },
  { key: Effect.FogDensity, label: 'adjustments.effects.fogDensity', initial: 50 },
  { key: Effect.FogHeight, label: 'adjustments.effects.fogHeight', initial: 0 },
  { key: Effect.FogVariation, label: 'adjustments.effects.fogVariation', initial: 25 },
  { key: Effect.FogGlow, label: 'adjustments.effects.glow', initial: 25 },
  {
    key: Effect.FogTemperature,
    label: 'adjustments.color.temperature',
    initial: 0,
    signed: true,
    track: 'temperature-gradient-track',
  },
  { key: Effect.FogTint, label: 'adjustments.color.tint', initial: 0, signed: true, track: 'tint-gradient-track' },
];

export default function FogPart({ adjustments, setAdjustments, onDragStateChange }: FogPartProps) {
  const t = useTheirText();

  const turnOn = useCallback(
    (map?: string) => setAdjustments((prev) => ({ ...prev, fogEnabled: true, fogDepthMap: map ?? prev.fogDepthMap })),
    [setAdjustments],
  );
  const turnOff = useCallback(
    () => setAdjustments((prev) => (prev.fogEnabled ? { ...prev, fogEnabled: false } : prev)),
    [setAdjustments],
  );
  const enabled = !!adjustments.fogEnabled;
  const hasMap = !!adjustments.fogDepthMap;
  const map = useEffectMap(DEPTH_MAP, { enabled, hasMap, turnOn, turnOff });

  const ready = enabled && hasMap && map.phase.kind === 'idle';

  return (
    <div>
      <Switch label={t('adjustments.effects.fog')} checked={map.isOn} onChange={map.toggle} />

      <MapStatus
        phase={map.phase}
        ask="fx166FogAsk"
        downloading="fx166FogDownloading"
        working={t('editor.ai.generatingDepthMap')}
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
            {SLIDERS.map(({ key, label, initial, signed, track }) => (
              <Slider
                key={key}
                label={t(label)}
                max={100}
                min={signed ? -100 : 0}
                defaultValue={initial}
                onChange={(e: SliderChangeEvent) =>
                  setAdjustments((prev) => ({ ...prev, [key]: parseInt(String(e.target.value), 10) }))
                }
                step={1}
                value={(adjustments[key as keyof Adjustments] as number | undefined) ?? initial}
                trackClassName={track}
                onDragStateChange={onDragStateChange}
                fillOrigin={signed ? undefined : 'min'}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
