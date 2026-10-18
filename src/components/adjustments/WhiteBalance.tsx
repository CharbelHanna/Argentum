// Argentum: white balance as Basic's first row, moved out of Color.tsx's White
// Balance section unchanged (RapidRAW 1.6.5 @ 79c2a46b): the K mode, the
// picker and the Temperature and Tint sliders are theirs. The row adds the
// label and the data-argentum="white-balance" slot, where the preset menu and
// Auto mount (argentum/WhiteBalanceMenu.tsx). A mask shows only its sliders.
import { Pipette } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import Slider, { SliderChangeEvent } from '../ui/Slider';
import { Adjustments } from '../../utils/adjustments';
import { AppSettings } from '../ui/AppProperties';
import { useSettingsStore } from '../../store/useSettingsStore';
import {
  getRelativeWhiteBalance,
  getWhiteBalanceMode,
  kelvinSliderScale,
  MAX_TEMPERATURE,
  MAX_TINT,
  MIN_TEMPERATURE,
  RELATIVE_RANGE,
  resolveWhiteBalance,
  WhiteBalance,
  WhiteBalanceMode,
  withKelvinWhiteBalance,
  withRelativeWhiteBalance,
} from '../../utils/whiteBalance';

interface WhiteBalanceControlsProps {
  adjustments: Adjustments;
  setAdjustments(adjustments: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)): void;
  appSettings?: AppSettings | null;
  asShotWhiteBalance?: WhiteBalance;
  isForMask?: boolean;
  isWbPickerActive?: boolean;
  toggleWbPicker?: () => void;
  onDragStateChange?: (isDragging: boolean) => void;
}

export default function WhiteBalanceControls({
  adjustments,
  setAdjustments,
  appSettings,
  asShotWhiteBalance,
  isForMask = false,
  isWbPickerActive = false,
  toggleWbPicker,
  onDragStateChange,
}: WhiteBalanceControlsProps) {
  const { t } = useTranslation();
  const handleSettingsChange = useSettingsStore((state) => state.handleSettingsChange);

  const kelvinAsShot =
    !isForMask && getWhiteBalanceMode(appSettings) === WhiteBalanceMode.Kelvin ? asShotWhiteBalance : undefined;
  const isKelvinMode = !!kelvinAsShot;
  const displayedWhiteBalance = kelvinAsShot
    ? resolveWhiteBalance(kelvinAsShot, adjustments)
    : getRelativeWhiteBalance(asShotWhiteBalance, adjustments);
  const tintRange = kelvinAsShot ? MAX_TINT : RELATIVE_RANGE;

  const toggleWhiteBalanceMode = () => {
    if (appSettings) {
      handleSettingsChange({
        ...appSettings,
        whiteBalanceMode: isKelvinMode ? WhiteBalanceMode.Relative : WhiteBalanceMode.Kelvin,
      });
    }
  };

  const handleWhiteBalanceChange = (key: keyof WhiteBalance, value: number | string) => {
    setAdjustments((prev: Adjustments) => {
      if (kelvinAsShot) {
        return withKelvinWhiteBalance(prev, { ...resolveWhiteBalance(kelvinAsShot, prev), [key]: Number(value) });
      }
      return prev.whiteBalance
        ? withRelativeWhiteBalance(prev, {
            ...getRelativeWhiteBalance(asShotWhiteBalance, prev),
            [key]: Number(value),
          })
        : { ...prev, [key]: Number(value) };
    });
  };

  return (
    <>
      {!isForMask && (
        <div className="flex items-center gap-1 mb-2" data-ag-wb-row>
          <span className="text-sm font-medium text-text-secondary select-none">
            {t('adjustments.color.whiteBalance')}
          </span>
          <span data-argentum="white-balance" className="ml-auto flex items-center gap-1 min-w-0" />
          <button
            onClick={toggleWhiteBalanceMode}
            className={`w-6 h-6 flex items-center justify-center rounded-md text-xs font-semibold transition-colors ${
              isKelvinMode ? 'bg-accent text-button-text' : 'hover:bg-bg-secondary text-text-secondary'
            }`}
            data-tooltip={t('adjustments.color.kelvinModeTooltip')}
          >
            K
          </button>
          {toggleWbPicker && (
            <button
              onClick={toggleWbPicker}
              className={`w-6 h-6 flex items-center justify-center rounded-md transition-colors ${
                isWbPickerActive ? 'bg-accent text-button-text' : 'hover:bg-bg-secondary text-text-secondary'
              }`}
              data-tooltip={t('adjustments.color.wbPickerTooltip')}
            >
              <Pipette size={16} />
            </button>
          )}
        </div>
      )}
      <Slider
        key={isKelvinMode ? 'kelvinTemperature' : 'temperature'}
        defaultValue={kelvinAsShot?.temperature}
        label={t('adjustments.color.temperature')}
        max={kelvinAsShot ? MAX_TEMPERATURE : RELATIVE_RANGE}
        min={kelvinAsShot ? MIN_TEMPERATURE : -RELATIVE_RANGE}
        onChange={(e: SliderChangeEvent) => handleWhiteBalanceChange('temperature', e.target.value)}
        scale={kelvinAsShot ? kelvinSliderScale : undefined}
        step={kelvinAsShot ? 50 : 1}
        suffix={kelvinAsShot ? 'K' : undefined}
        value={displayedWhiteBalance.temperature}
        trackClassName="temperature-gradient-track"
        onDragStateChange={onDragStateChange}
      />
      <Slider
        key={isKelvinMode ? 'kelvinTint' : 'tint'}
        defaultValue={kelvinAsShot?.tint}
        label={t('adjustments.color.tint')}
        max={tintRange}
        min={-tintRange}
        onChange={(e: SliderChangeEvent) => handleWhiteBalanceChange('tint', e.target.value)}
        step={1}
        value={displayedWhiteBalance.tint}
        trackClassName="tint-gradient-track"
        onDragStateChange={onDragStateChange}
      />
    </>
  );
}
