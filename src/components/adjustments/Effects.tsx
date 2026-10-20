import { useState, useEffect, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Loader2, Circle, Hexagon, Octagon, Aperture } from 'lucide-react';
import { motion } from 'framer-motion';
import clsx from 'clsx';
import Slider from '../ui/Slider';
import Switch from '../ui/Switch';
import {
  Adjustments,
  Effect,
  CreativeAdjustment,
  getAdjustmentToolOrder,
  getHiddenAdjustmentTools,
} from '../../utils/adjustments';
import { AppSettings } from '../ui/AppProperties';
import Text from '../ui/Text';
import AdjustmentSubSection from './AdjustmentSubSection';
import { TextVariants } from '../../types/typography';
import { DepthRangePicker } from '../ui/DepthRangePicker';
import { useProcessStore } from '../../store/useProcessStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { useEditorStore } from '../../store/useEditorStore';
// Argentum: Relight and Fog as parts of their own (RapidRAW 1.6.6 has them in "Spatial").
import RelightPart from '../../argentum/effects/RelightPart';
import FogPart from '../../argentum/effects/FogPart';
import { useAgTranslation } from '../../argentum/locales';
import { useTheirText } from '../../argentum/effects/theirText';

// Argentum: the LUT block moved to the Looks tab (argentum/looks/LooksPanel.tsx).
interface EffectsPanelProps {
  adjustments: Adjustments;
  isForMask?: boolean;
  setAdjustments(adjustments: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)): any;
  appSettings: AppSettings | null;
  onDragStateChange?: (isDragging: boolean) => void;
}

interface BokehShapeSwitchProps {
  selectedShape: string;
  onShapeChange: (shape: string) => void;
}

const BokehShapeSwitch = ({ selectedShape, onShapeChange }: BokehShapeSwitchProps) => {
  const { t } = useTranslation();
  const [bubbleStyle, setBubbleStyle] = useState({});
  const [isLabelHovered, setIsLabelHovered] = useState(false);
  const isInitialAnimation = useRef(true);

  const shapeOptions = useMemo(
    () => [
      { id: 'circle', icon: Circle, title: t('adjustments.effects.bokehCircular') },
      { id: 'hexagon', icon: Hexagon, title: t('adjustments.effects.bokehHexagonal') },
      { id: 'octagon', icon: Octagon, title: t('adjustments.effects.bokehOctagonal') },
      { id: 'ring', icon: Aperture, title: t('adjustments.effects.bokehRing') },
    ],
    [t],
  );

  useEffect(() => {
    const selectedIndex = shapeOptions.findIndex((m) => m.id === selectedShape);
    const safeIndex = selectedIndex >= 0 ? selectedIndex : 0;

    const widthPercent = 100 / shapeOptions.length;
    const targetX = `${safeIndex * 100}%`;
    const targetWidth = `${widthPercent}%`;

    if (isInitialAnimation.current) {
      setBubbleStyle({
        x: ['-25%', targetX],
        width: targetWidth,
      });
      isInitialAnimation.current = false;
    } else {
      setBubbleStyle({
        x: targetX,
        width: targetWidth,
      });
    }
  }, [selectedShape, shapeOptions]);

  const handleReset = () => {
    onShapeChange('circle');
  };

  return (
    <div className="flex flex-col gap-2 mt-3">
      <div
        className="grid w-fit cursor-pointer"
        onClick={handleReset}
        onMouseEnter={() => setIsLabelHovered(true)}
        onMouseLeave={() => setIsLabelHovered(false)}
      >
        <Text
          variant={TextVariants.label}
          aria-hidden={isLabelHovered}
          className={`col-start-1 row-start-1 text-text-secondary select-none transition-opacity duration-200 ease-in-out ${
            isLabelHovered ? 'opacity-0' : 'opacity-100'
          }`}
        >
          {t('adjustments.effects.bokehShape')}
        </Text>
        <Text
          variant={TextVariants.label}
          aria-hidden={!isLabelHovered}
          className={`col-start-1 row-start-1 text-accent! select-none transition-opacity duration-200 ease-in-out pointer-events-none ${
            isLabelHovered ? 'opacity-100' : 'opacity-0'
          }`}
        >
          {t('ui.slider.reset')}
        </Text>
      </div>

      <div className="w-full p-1 bg-bg-primary rounded-md">
        <div className="relative flex w-full">
          <motion.div
            className="absolute top-0 bottom-0 z-0 bg-accent"
            style={{ borderRadius: 6 }}
            animate={bubbleStyle}
            transition={{ type: 'spring', bounce: 0.2, duration: 0.6 }}
          />
          {shapeOptions.map((shape) => {
            const Icon = shape.icon;
            return (
              <button
                key={shape.id}
                data-tooltip={shape.title}
                onClick={() => onShapeChange(shape.id)}
                className={clsx(
                  'relative flex-1 flex items-center justify-center gap-2 px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
                  {
                    'text-text-secondary hover:text-text-primary hover:bg-surface': selectedShape !== shape.id,
                    'text-button-text': selectedShape === shape.id,
                  },
                )}
                style={{ WebkitTapHighlightColor: 'transparent' }}
              >
                <span className="relative z-10 flex items-center">
                  <Icon size={16} strokeWidth={2} />
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
};

export default function EffectsPanel({
  adjustments,
  setAdjustments,
  isForMask = false,
  appSettings,
  onDragStateChange,
}: EffectsPanelProps) {
  const { t } = useTranslation();
  const [isGeneratingDepth, setIsGeneratingDepth] = useState(false);
  const aiModelDownloadStatus = useProcessStore((state) => state.aiModelDownloadStatus);

  const handleGenerateLensBlurDepthMap = async () => {
    setIsGeneratingDepth(true);
    try {
      const b64: string = await invoke('generate_full_image_depth_map', { jsAdjustments: adjustments });
      setAdjustments((prev: Partial<Adjustments>) => ({
        ...prev,
        lensBlurDepthMap: b64,
      }));
    } catch (e: any) {
      toast.error(`Failed to generate depth map: ${e}`);
      setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, lensBlurEnabled: false }));
    } finally {
      setIsGeneratingDepth(false);
    }
  };

  const handleAdjustmentChange = (key: string, value: any) => {
    const numericValue = typeof value === 'boolean' ? value : parseInt(value, 10);
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: numericValue }));
  };

  const handleLensBlurToggle = (enabled: boolean) => {
    handleAdjustmentChange(Effect.LensBlurEnabled, enabled);
    if (enabled && !adjustments.lensBlurDepthMap) {
      handleGenerateLensBlurDepthMap();
    }
  };

  const isAiFree = useSettingsStore((s) => s.appSettings?.aiProvider === 'ai-free');
  const hiddenTools = getHiddenAdjustmentTools(appSettings?.adjustmentLayout);
  // Argentum: RapidRAW 1.6.6 lists Lens blur, Relight and Fog as one tool, 'spatial', in the
  // Customise menu; an order saved before then names 'lensBlur', which stands for it.
  const savedOrder = appSettings?.adjustmentLayout?.toolOrder?.effects;
  const toolOrder = getAdjustmentToolOrder(
    'effects',
    savedOrder && { effects: savedOrder.map((id) => (id === 'lensBlur' ? 'spatial' : id)) },
  );
  const spatialOrder = toolOrder.indexOf('spatial');
  const isSpatialHidden = hiddenTools.includes('spatial');
  const ag = useAgTranslation();
  const theirs = useTheirText();
  const setEditor = useEditorStore((s) => s.setEditor);
  const onPart = (enabled: boolean | undefined) =>
    enabled ? <span className="text-xs text-accent select-none">{ag('fx166On')}</span> : undefined;

  return (
    <div className="flex flex-col gap-4">
      {!hiddenTools.includes('creative') && (
        <AdjustmentSubSection
          id="creative"
          order={toolOrder.indexOf('creative')}
          title={t('adjustments.effects.creative')}
        >
          <Slider
            label={t('adjustments.effects.glow')}
            max={100}
            min={0}
            onChange={(e: any) => handleAdjustmentChange(CreativeAdjustment.GlowAmount, e.target.value)}
            step={1}
            value={adjustments.glowAmount}
            onDragStateChange={onDragStateChange}
          />

          <Slider
            label={t('adjustments.effects.halation')}
            max={100}
            min={0}
            onChange={(e: any) => handleAdjustmentChange(CreativeAdjustment.HalationAmount, e.target.value)}
            step={1}
            value={adjustments.halationAmount}
            onDragStateChange={onDragStateChange}
          />

          {!isForMask && (
            <Slider
              label={t('adjustments.effects.lightFlares')}
              max={100}
              min={0}
              onChange={(e: any) => handleAdjustmentChange(CreativeAdjustment.FlareAmount, e.target.value)}
              step={1}
              value={adjustments.flareAmount}
              onDragStateChange={onDragStateChange}
            />
          )}
        </AdjustmentSubSection>
      )}

      {!isForMask && (
        <>
          {!isAiFree && !isSpatialHidden && !hiddenTools.includes('lensBlur') && (
            <AdjustmentSubSection id="lensBlur" order={spatialOrder} title={t('adjustments.effects.lensBlur')}>
              <Switch
                label={t('adjustments.effects.lensBlur')}
                checked={!!adjustments.lensBlurEnabled}
                onChange={handleLensBlurToggle}
              />

              <div
                className={`grid transition-all duration-300 ease-in-out ${
                  adjustments.lensBlurEnabled ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'
                }`}
              >
                <div className="overflow-hidden">
                  <div className="space-y-4 pt-4 pb-1">
                    {isGeneratingDepth ? (
                      <div className="flex flex-col items-center justify-center gap-1 p-4 text-text-secondary text-center">
                        <div className="flex items-center gap-2">
                          <Loader2 size={16} className="animate-spin shrink-0" />
                          <Text variant={TextVariants.label}>
                            {aiModelDownloadStatus
                              ? t('editor.masks.settings.aiModelDownloading')
                              : t('editor.ai.generatingDepthMap')}
                          </Text>
                        </div>
                        {aiModelDownloadStatus && (
                          <Text variant={TextVariants.small} className="text-accent">
                            {aiModelDownloadStatus}
                          </Text>
                        )}
                      </div>
                    ) : (
                      <>
                        <Slider
                          label={t('adjustments.effects.amount')}
                          max={100}
                          min={0}
                          defaultValue={40}
                          onChange={(e: any) => handleAdjustmentChange(Effect.LensBlurAmount, e.target.value)}
                          step={1}
                          value={adjustments.lensBlurAmount ?? 50}
                          onDragStateChange={onDragStateChange}
                          fillOrigin="min"
                        />

                        <Slider
                          label={t('adjustments.effects.lensDiffusion')}
                          max={100}
                          min={0}
                          defaultValue={0}
                          onChange={(e: any) => handleAdjustmentChange(Effect.lensBlurDiffusion, e.target.value)}
                          step={1}
                          value={adjustments.lensBlurDiffusion ?? 0}
                          onDragStateChange={onDragStateChange}
                        />

                        <BokehShapeSwitch
                          selectedShape={adjustments.lensBlurShape || 'circle'}
                          onShapeChange={(shapeId) =>
                            setAdjustments((prev: Partial<Adjustments>) => ({
                              ...prev,
                              [Effect.LensBlurShape]: shapeId,
                            }))
                          }
                        />

                        <DepthRangePicker
                          minDepth={100 - (adjustments.lensBlurMaxDepth ?? 100)}
                          maxDepth={100 - (adjustments.lensBlurMinDepth ?? 20)}
                          minFade={adjustments.lensBlurMaxFade ?? 20}
                          maxFade={adjustments.lensBlurMinFade ?? 20}
                          defaultMinDepth={0}
                          defaultMaxDepth={80}
                          defaultMinFade={20}
                          defaultMaxFade={20}
                          onChange={(values: {
                            minDepth: number;
                            maxDepth: number;
                            minFade: number;
                            maxFade: number;
                          }) => {
                            setAdjustments((prev: Partial<Adjustments>) => ({
                              ...prev,
                              lensBlurMinDepth: 100 - values.maxDepth,
                              lensBlurMaxDepth: 100 - values.minDepth,
                              lensBlurMinFade: values.maxFade,
                              lensBlurMaxFade: values.minFade,
                            }));
                          }}
                          onDragStateChange={onDragStateChange}
                        />
                      </>
                    )}
                  </div>
                </div>
              </div>
            </AdjustmentSubSection>
          )}

          {!isAiFree && !isSpatialHidden && (
            <AdjustmentSubSection
              id="relight"
              order={spatialOrder}
              title={theirs('adjustments.effects.relight')}
              actions={onPart(adjustments.relightEnabled)}
            >
              <RelightPart
                adjustments={adjustments}
                setAdjustments={setAdjustments}
                onDragStateChange={onDragStateChange}
              />
            </AdjustmentSubSection>
          )}

          {!isAiFree && !isSpatialHidden && (
            <AdjustmentSubSection
              id="fog"
              order={spatialOrder}
              title={theirs('adjustments.effects.fog')}
              actions={onPart(adjustments.fogEnabled)}
            >
              <FogPart
                adjustments={adjustments}
                setAdjustments={setAdjustments}
                onDragStateChange={onDragStateChange}
              />
            </AdjustmentSubSection>
          )}

          {!hiddenTools.includes('vignette') && (
            <AdjustmentSubSection
              id="vignette"
              order={toolOrder.indexOf('vignette')}
              title={t('adjustments.effects.vignette')}
            >
              {/* Argentum: touching these shows the pin that moves the vignette's centre (argentum/effects/onPhoto.tsx). */}
              <div
                data-center-marker-controls="vignette"
                data-tooltip={ag('fx166CentreTip')}
                onPointerDown={() =>
                  setEditor({ activeCenterMarker: 'vignette', isRelightPickerActive: false, isWbPickerActive: false })
                }
              >
                <Slider
                  label={t('adjustments.effects.amount')}
                  max={100}
                  min={-100}
                  onChange={(e: any) => handleAdjustmentChange(Effect.VignetteAmount, e.target.value)}
                  step={1}
                  value={adjustments.vignetteAmount}
                  onDragStateChange={onDragStateChange}
                />
                <Slider
                  defaultValue={50}
                  label={t('adjustments.effects.midpoint')}
                  max={100}
                  min={0}
                  onChange={(e: any) => handleAdjustmentChange(Effect.VignetteMidpoint, e.target.value)}
                  step={1}
                  value={adjustments.vignetteMidpoint}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  label={t('adjustments.effects.roundness')}
                  max={100}
                  min={-100}
                  onChange={(e: any) => handleAdjustmentChange(Effect.VignetteRoundness, e.target.value)}
                  step={1}
                  value={adjustments.vignetteRoundness}
                  onDragStateChange={onDragStateChange}
                />
                <Slider
                  defaultValue={50}
                  label={t('adjustments.effects.feather')}
                  max={100}
                  min={0}
                  onChange={(e: any) => handleAdjustmentChange(Effect.VignetteFeather, e.target.value)}
                  step={1}
                  value={adjustments.vignetteFeather}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
              </div>
            </AdjustmentSubSection>
          )}

          {!hiddenTools.includes('grain') && (
            <AdjustmentSubSection id="grain" order={toolOrder.indexOf('grain')} title={t('adjustments.effects.grain')}>
              <Slider
                label={t('adjustments.effects.amount')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.GrainAmount, e.target.value)}
                step={1}
                value={adjustments.grainAmount}
                onDragStateChange={onDragStateChange}
              />
              <Slider
                defaultValue={25}
                label={t('adjustments.effects.size')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.GrainSize, e.target.value)}
                step={1}
                value={adjustments.grainSize}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
              <Slider
                defaultValue={50}
                label={t('adjustments.effects.roughness')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.GrainRoughness, e.target.value)}
                step={1}
                value={adjustments.grainRoughness}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
            </AdjustmentSubSection>
          )}
        </>
      )}
    </div>
  );
}
