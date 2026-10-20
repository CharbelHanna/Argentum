import { motion } from 'framer-motion';
import clsx from 'clsx';
import Slider from '../ui/Slider';
import { Adjustments, BasicAdjustment, ColorAdjustment, DetailsAdjustment } from '../../utils/adjustments';
import { WhiteBalance } from '../../utils/whiteBalance';
import { useEffect, useRef, useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import WhiteBalanceControls from './WhiteBalance';
import { useEditorStore } from '../../store/useEditorStore';
import { useAgTranslation } from '../../argentum/locales';
import { NO_EFFECT_IN_BLACK_WHITE, TreatmentSwitch, isBlackWhite } from '../../argentum/BlackWhite';

interface BasicAdjustmentsProps {
  adjustments: Adjustments;
  setAdjustments(adjustments: Partial<Adjustments>): any;
  isForMask?: boolean;
  onDragStateChange?: (isDragging: boolean) => void;
  appSettings?: any;
  // Argentum: for white balance, now Basic's first row (WhiteBalance.tsx).
  asShotWhiteBalance?: WhiteBalance;
  isWbPickerActive?: boolean;
  toggleWbPicker?: () => void;
}

interface ToneMapperSwitchProps {
  selectedMapper: string;
  onMapperChange: (mapper: string) => void;
}

// Argentum: shown in the Profile section (argentum/ProfileSection.tsx), without
// the Exposure slider it used to hold, which is back among Basic's sliders.
export const ToneMapperSwitch = ({ selectedMapper, onMapperChange }: ToneMapperSwitchProps) => {
  const { t } = useTranslation();
  const [bubbleStyle, setBubbleStyle] = useState({});
  const isInitialAnimation = useRef(true);
  const [isLabelHovered, setIsLabelHovered] = useState(false);

  const toneMapperOptions = useMemo(
    () => [
      {
        id: 'basic',
        label: t('adjustments.basic.mappers.basic'),
        title: t('adjustments.basic.mappers.basicDesc'),
      },
      {
        id: 'agx',
        label: t('adjustments.basic.mappers.agx'),
        title: t('adjustments.basic.mappers.agxDesc'),
      },
    ],
    [t],
  );

  const handleReset = () => {
    onMapperChange('basic');
  };

  useEffect(() => {
    const selectedIndex = toneMapperOptions.findIndex((m) => m.id === selectedMapper);
    const safeIndex = selectedIndex >= 0 ? selectedIndex : 0;

    const widthPercent = 100 / toneMapperOptions.length;
    const targetX = `${safeIndex * 100}%`;
    const targetWidth = `${widthPercent}%`;

    if (isInitialAnimation.current) {
      let initialX;
      if (selectedMapper === 'agx') {
        initialX = `${toneMapperOptions.length * 100}%`;
      } else {
        initialX = '-25%';
      }

      setBubbleStyle({
        x: [initialX, targetX],
        width: targetWidth,
      });
      isInitialAnimation.current = false;
    } else {
      setBubbleStyle({
        x: targetX,
        width: targetWidth,
      });
    }
  }, [selectedMapper, toneMapperOptions]);

  return (
    <div className="group flex items-center gap-3">
      <div className="flex items-center shrink-0">
        <div
          className="grid cursor-pointer"
          onClick={handleReset}
          onDoubleClick={handleReset}
          onMouseEnter={() => setIsLabelHovered(true)}
          onMouseLeave={() => setIsLabelHovered(false)}
        >
          <span
            aria-hidden={isLabelHovered}
            className={`col-start-1 row-start-1 text-sm font-medium text-text-secondary select-none transition-opacity duration-200 ease-in-out ${
              isLabelHovered ? 'opacity-0' : 'opacity-100'
            }`}
          >
            {t('adjustments.basic.toneMapper')}
          </span>
          <span
            aria-hidden={!isLabelHovered}
            className={`col-start-1 row-start-1 text-sm font-medium text-text-primary select-none transition-opacity duration-200 ease-in-out pointer-events-none ${
              isLabelHovered ? 'opacity-100' : 'opacity-0'
            }`}
          >
            {t('adjustments.basic.reset')}
          </span>
        </div>
      </div>
      <div className="flex-1 min-w-0 p-0.5 bg-card-active rounded-md">
        <div className="relative flex w-full">
          <motion.div
            className="absolute top-0 bottom-0 z-0 bg-accent"
            style={{ borderRadius: 6 }}
            animate={bubbleStyle}
            transition={{ type: 'spring', bounce: 0.2, duration: 0.6 }}
          />
          {toneMapperOptions.map((mapper) => (
            <button
              key={mapper.id}
              data-tooltip={mapper.title}
              onClick={() => onMapperChange(mapper.id)}
              className={clsx(
                'relative flex-1 flex items-center justify-center gap-2 px-2 py-1 text-sm font-medium rounded-md transition-colors',
                {
                  'text-text-primary hover:bg-surface': selectedMapper !== mapper.id,
                  'text-button-text': selectedMapper === mapper.id,
                },
              )}
              style={{ WebkitTapHighlightColor: 'transparent' }}
            >
              <span className="relative z-10 flex items-center">{mapper.label}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};

// Argentum: a small heading between groups of sliders, as Lightroom's Basic has.
const Group = ({ children }: { children: string }) => (
  <div className="pt-2 pb-1 text-center text-xs text-text-secondary select-none">{children}</div>
);

// Argentum: Basic is white balance, then Tone, then Presence: the corrections
// most photos need, in the order they are made. White balance, Vibrance and
// Saturation came from Color, Clarity, Dehaze, Structure and Centre from
// Details; their section eye, reset, copy and paste moved with them
// (argentum/editSections.ts, and the renderer's section checks). The tone
// mapper went to Profile.
export default function BasicAdjustments({
  adjustments,
  setAdjustments,
  isForMask = false,
  onDragStateChange,
  appSettings,
  asShotWhiteBalance,
  isWbPickerActive,
  toggleWbPicker,
}: BasicAdjustmentsProps) {
  const { t } = useTranslation();
  const ag = useAgTranslation();
  const setEditor = useEditorStore((s) => s.setEditor);

  const handleAdjustmentChange = (key: BasicAdjustment | ColorAdjustment | DetailsAdjustment, value: any) => {
    const numericValue = parseFloat(value);
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: numericValue }));
  };

  const slider = (key: BasicAdjustment | ColorAdjustment | DetailsAdjustment, label: string, range = 100) => (
    <Slider
      key={key}
      label={label}
      max={range}
      min={-range}
      onChange={(e: any) => handleAdjustmentChange(key, e.target.value)}
      step={range === 100 ? 1 : 0.01}
      value={(adjustments as any)[key] || 0}
      onDragStateChange={onDragStateChange}
    />
  );

  return (
    <div>
      {/* Argentum: Color | B&W, as Lightroom's Treatment (argentum/BlackWhite.tsx). */}
      {!isForMask && <TreatmentSwitch adjustments={adjustments} setAdjustments={setAdjustments} />}
      <WhiteBalanceControls
        adjustments={adjustments}
        setAdjustments={setAdjustments}
        appSettings={appSettings}
        asShotWhiteBalance={asShotWhiteBalance}
        isForMask={isForMask}
        isWbPickerActive={isWbPickerActive}
        toggleWbPicker={toggleWbPicker}
        onDragStateChange={onDragStateChange}
      />
      <Group>{t('adjustments.basic.toneGroup')}</Group>
      {slider(BasicAdjustment.Exposure, t('adjustments.basic.exposure'), 5)}
      {slider(BasicAdjustment.Contrast, t('adjustments.basic.contrast'))}
      {slider(BasicAdjustment.Highlights, t('adjustments.basic.highlights'))}
      {slider(BasicAdjustment.Shadows, t('adjustments.basic.shadows'))}
      {slider(BasicAdjustment.Whites, t('adjustments.basic.whites'))}
      {slider(BasicAdjustment.Blacks, t('adjustments.basic.blacks'))}
      {slider(BasicAdjustment.Brightness, t('adjustments.basic.brightness'), 5)}
      <Group>{t('adjustments.color.presence')}</Group>
      {slider(DetailsAdjustment.Clarity, t('adjustments.details.clarity'))}
      {slider(DetailsAdjustment.Dehaze, t('adjustments.details.dehaze'))}
      {slider(DetailsAdjustment.Structure, t('adjustments.details.structure'))}
      {/* Argentum: touching Centre shows the pin that moves its centre (argentum/effects/onPhoto.tsx). */}
      {!isForMask && (
        <div
          data-center-marker-controls="centre"
          data-tooltip={ag('fx166CentreTip')}
          onPointerDown={() =>
            setEditor({ activeCenterMarker: 'centre', isRelightPickerActive: false, isWbPickerActive: false })
          }
        >
          {slider(DetailsAdjustment.Centré, t('adjustments.details.centre'))}
        </div>
      )}
      <div className={clsx(!isForMask && isBlackWhite(adjustments) && NO_EFFECT_IN_BLACK_WHITE)}>
        {slider(ColorAdjustment.Vibrance, t('adjustments.color.vibrance'))}
        {slider(ColorAdjustment.Saturation, t('adjustments.color.saturation'))}
      </div>
    </div>
  );
}
