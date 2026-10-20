/**
 * B&W: Lightroom's Treatment switch, Color or B&W, at the top of Edit > Basic,
 * and its B&W Mix, which takes the Color Mixer's place in Edit > Color while
 * the photo is black and white. Ours.
 *
 * The edit's `agBlackWhite` holds both: `enabled`, and each band's -100..100,
 * how light that colour turns in the grey. The renderer turns the photo grey
 * after Saturation and before Color Grading, with RAWmakase's measured mix, and
 * leaves the Color Mixer, Hue, Saturation and Vibrance out while B&W is on
 * (mods/black_white.rs); they are kept for when the photo goes back to Color.
 * V switches, as in Lightroom and RAWmakase.
 */

import { useEffect } from 'react';
import { motion } from 'framer-motion';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import Slider from '../components/ui/Slider';
import type { Adjustments } from '../utils/adjustments';
import { useAgTranslation } from './locales';
import './blackWhite.css';

export const BLACK_WHITE_KEY = 'agBlackWhite';

/**
 * The mix's bands, in Lightroom's order, named as the Color Mixer names them.
 * Each slider's track runs from black through the band's colour to white
 * (blackWhite.css).
 */
const BANDS = ['reds', 'oranges', 'yellows', 'greens', 'aquas', 'blues', 'purples', 'magentas'] as const;

type Band = (typeof BANDS)[number];
type BlackWhite = { enabled: boolean } & Partial<Record<Band, number>>;
type Edit = Partial<Adjustments> & { [BLACK_WHITE_KEY]?: BlackWhite };

/** A control greyed out while B&W is on, as Lightroom greys them: it has no effect. */
export const NO_EFFECT_IN_BLACK_WHITE = 'opacity-40 grayscale pointer-events-none';

export const isBlackWhite = (adjustments: unknown): boolean =>
  (adjustments as Edit | null)?.[BLACK_WHITE_KEY]?.enabled === true;

function withBlackWhite(prev: Edit, change: Partial<BlackWhite>): Edit {
  const current = prev?.[BLACK_WHITE_KEY] ?? { enabled: false };
  return { ...prev, [BLACK_WHITE_KEY]: { ...current, ...change } };
}

const typingInto = (target: EventTarget | null) =>
  target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));

/** As the panels pass them: the setter also takes an update of the previous edit. */
interface Props {
  adjustments: Adjustments;
  setAdjustments(adjustments: Partial<Adjustments>): unknown;
}

/** Color | B&W, at the top of Basic. */
export function TreatmentSwitch({ adjustments, setAdjustments }: Props) {
  const ag = useAgTranslation();
  const blackWhite = isBlackWhite(adjustments);
  const choose = (enabled: boolean) => setAdjustments((prev: Edit) => withBlackWhite(prev, { enabled }));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() !== 'v' || e.ctrlKey || e.metaKey || e.altKey || e.repeat || typingInto(e.target)) {
        return;
      }
      e.preventDefault();
      setAdjustments((prev: Edit) => withBlackWhite(prev, { enabled: !isBlackWhite(prev) }));
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [setAdjustments]);

  const option = (enabled: boolean, label: string) => (
    <button
      onClick={() => choose(enabled)}
      className={clsx(
        'relative flex-1 flex items-center justify-center px-3 p-1.5 text-sm font-medium rounded-md transition-colors',
        blackWhite === enabled ? 'text-button-text' : 'text-text-primary hover:bg-surface/50',
      )}
      style={{ WebkitTapHighlightColor: 'transparent' }}
    >
      <span className="relative z-10">{label}</span>
    </button>
  );

  return (
    <div className="w-full p-1 bg-card-active rounded-md mb-3" data-tooltip={ag('bwSwitchTip')}>
      <div className="relative flex w-full">
        <motion.div
          className="absolute top-0 bottom-0 z-0 bg-accent"
          style={{ borderRadius: 6, width: '50%' }}
          animate={{ x: blackWhite ? '100%' : '0%' }}
          transition={{ type: 'spring', bounce: 0.2, duration: 0.6 }}
        />
        {option(false, ag('bwColor'))}
        {option(true, ag('bwBlackWhite'))}
      </div>
    </div>
  );
}

interface MixProps extends Props {
  onDragStateChange?(isDragging: boolean): void;
}

/** B&W Mix's Reset, in its heading: every band back to 0, B&W left on. */
export function BlackWhiteMixReset({ setAdjustments }: Pick<Props, 'setAdjustments'>) {
  const ag = useAgTranslation();
  const reset = () =>
    setAdjustments((prev: Edit) => withBlackWhite(prev, Object.fromEntries(BANDS.map((band) => [band, 0]))));
  return (
    <button className="text-xs text-text-secondary hover:text-text-primary px-1" onClick={reset}>
      {ag('bwMixReset')}
    </button>
  );
}

/** B&W Mix: how light each colour turns, -100..100. */
export function BlackWhiteMix({ adjustments, setAdjustments, onDragStateChange }: MixProps) {
  const { t } = useTranslation();
  const mix: BlackWhite = (adjustments as Edit)[BLACK_WHITE_KEY] ?? { enabled: true };

  return (
    <div>
      {BANDS.map((band) => (
        <Slider
          key={band}
          label={t(`adjustments.color.mixerColors.${band}`)}
          min={-100}
          max={100}
          step={1}
          value={mix[band] ?? 0}
          onChange={(e) => {
            const value = parseFloat(String(e.target.value));
            setAdjustments((prev: Edit) => withBlackWhite(prev, { [band]: value }));
          }}
          trackClassName={`ag-bw-track-${band}`}
          onDragStateChange={onDragStateChange}
        />
      ))}
    </div>
  );
}
