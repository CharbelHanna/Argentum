/**
 * Settings > General > Panels: the text size and the row spacing. Ours.
 *
 * It belongs in their General card, under Font, with the other things that
 * change how the app looks. Their settings file allows Argentum one line, the
 * slot at the end of the page, so the row is placed from our side the way AI
 * Models is: a container of ours goes in after their Font row, and the controls
 * are portalled into it. Their card spaces its rows itself, so ours lines up.
 *
 * Their Font row is found by its label, read from their own translations, so
 * it matches in every language. If it is ever not found, the row goes in the
 * slot at the end of the page, in a card of its own, rather than nowhere.
 *
 * It took the place of Compact panels, a switch and its Row spacing, when the
 * panels became compact for good.
 */

import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import clsx from 'clsx';
import Slider from '../components/ui/Slider';
import Text from '../components/ui/Text';
import { TextVariants } from '../types/typography';
import { DEFAULT_GAP, MAX_GAP, useCompactSliders } from './compactSliders';
import { type TextSizeName, useEditorLayout } from './layout/editorLayout';
import { type AgKey, useAgTranslation } from './locales';

const TEXT_SIZES: { id: TextSizeName; label: AgKey }[] = [
  { id: 'small', label: 'panelsTextSmall' },
  { id: 'normal', label: 'panelsTextNormal' },
  { id: 'large', label: 'panelsTextLarge' },
];

function PanelsRow() {
  const t = useAgTranslation();
  const gap = useCompactSliders((s) => s.gap);
  const chooseGap = useCompactSliders((s) => s.chooseGap);
  const textSize = useEditorLayout((s) => s.textSize);
  const chooseTextSize = useEditorLayout((s) => s.chooseTextSize);

  // Their SettingItem's markup: it is not exported, and the row has to look
  // like the ones around it.
  return (
    <div>
      <Text variant={TextVariants.heading} className="block mb-2">
        {t('panelsSetting')}
      </Text>
      <div className="flex items-center gap-3">
        <span className="text-sm text-text-secondary select-none">{t('panelsTextSize')}</span>
        <div className="flex gap-1 p-0.5 rounded-md bg-bg-primary" role="radiogroup">
          {TEXT_SIZES.map((size) => (
            <button
              key={size.id}
              role="radio"
              aria-checked={textSize === size.id}
              onClick={() => chooseTextSize(size.id)}
              className={clsx(
                'px-3 py-1 rounded text-sm transition-colors',
                textSize === size.id
                  ? 'bg-accent text-button-text'
                  : 'text-text-secondary hover:text-text-primary hover:bg-surface',
              )}
            >
              {t(size.label)}
            </button>
          ))}
        </div>
      </div>
      {/* Their slider, so it is itself drawn compact and shows the spacing it sets as it is dragged. */}
      <div className="mt-3">
        <Slider
          label={t('compactSlidersGap')}
          min={0}
          max={MAX_GAP}
          step={1}
          defaultValue={DEFAULT_GAP}
          value={gap}
          suffix="px"
          onChange={(e) => chooseGap(Number(e.target.value))}
        />
      </div>
      <Text variant={TextVariants.small} className="mt-2">
        {t('panelsDesc')}
      </Text>
    </div>
  );
}

export default function PanelsSetting({ slot }: { slot: HTMLElement }) {
  const { t } = useTranslation();
  const [host, setHost] = useState<HTMLElement | null>(null);

  useEffect(() => {
    const container = slot.parentElement;
    if (!container) return;
    const font = t('settings.general.font');
    const own = document.createElement('div');

    const place = () => {
      // Their General page is the element just before the slot, while it is
      // showing. Until it arrives there is nothing to place against.
      const page = slot.previousElementSibling;
      if (!page) return;
      // Their SettingItem is a div whose first child is its label.
      const row = Array.from(page.querySelectorAll('div')).find(
        (el) => el !== own && el.firstElementChild?.textContent?.trim() === font,
      );
      if (row) {
        // Inside their card, which spaces its rows itself.
        own.className = '';
        if (row.nextElementSibling !== own) row.after(own);
      } else if (own.parentElement !== slot) {
        // Outside their page, so it needs a card and the gap their cards have.
        own.className = 'mt-10 p-6 bg-surface rounded-xl shadow-md';
        slot.appendChild(own);
      }
      setHost(own);
    };

    place();
    const observer = new MutationObserver(place);
    observer.observe(container, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      own.remove();
    };
  }, [slot, t]);

  return host ? createPortal(<PanelsRow />, host) : null;
}
