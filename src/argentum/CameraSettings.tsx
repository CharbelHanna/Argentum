/**
 * How the picture was taken, shown over the Analytics scope. Ours.
 *
 * WHY IT IS HERE AND NOT IN THE METADATA PANEL
 *
 * The scope answers "is this exposure right?". The shutter speed answers "why".
 * Those two lived in different panels, so judging an exposure meant switching
 * away from the very thing being judged. ISO, aperture, shutter and focal
 * length belong next to the histogram they explain.
 *
 * NOTHING IS RETRIEVED HERE
 *
 * All six values are already decoded by `extract_metadata` in
 * `src-tauri/src/exif_processing.rs` and already sit in
 * `useEditorStore.selectedImage.exif`. This reads that. A Tauri command would
 * be a round trip to fetch a string the store is holding.
 *
 * WHY IT FLOATS INSTEAD OF SITTING IN THE LAYOUT
 *
 * The Analytics band is upstream's (`ControlsPanel.tsx`) and carries no
 * `data-argentum` marker, so laying this out beside the scope would cost a
 * permanent anchor in one of their files — see `Argentum.tsx` for why that
 * budget is not spent lightly. Instead this measures their band and floats over
 * it, costing them nothing.
 *
 * The honest trade: it covers a little of the waveform, and its position is
 * computed rather than laid out. Mitigated by sitting top-left (the scope's own
 * mode buttons appear across the bottom on hover) and by `pointer-events-none`
 * throughout. If it ever needs to become a real layout child, add
 * `<div data-argentum="analytics" />` to `ControlsPanel.tsx` and swap the
 * locator for `useAnchor`; nothing below this comment would change.
 */

import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Camera } from 'lucide-react';
import { IconAperture, IconFocalLength, IconIso, IconLens, IconShutter } from '../components/panel/editor/ExifIcons';
import { Panel } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { useAgTranslation } from './locales';

/** The band's own padding, so the strip lines up with the scope's corner. */
const BAND_PADDING = 12;
const INSET = 6;

/**
 * Their formatters, copied.
 *
 * `KEY_CAMERA_SETTINGS_MAP` in `MetadataPanel.tsx` is a non-exported const;
 * exporting it would be a line in a file of theirs. Copied rather than
 * re-derived so the two panels cannot disagree about how an aperture is
 * written — if upstream changes theirs, the registry entry is what says to
 * come back here.
 *
 * Every one is idempotent, because Rust already writes most of the unit:
 * `"f/2.8"`, `"1/250 s"`. Appending blindly would give `f/f/2.8`.
 */
const formatAperture = (value: unknown): string => {
  const s = String(value);
  return s.toLowerCase().startsWith('f') ? s : `f/${s}`;
};

const formatShutter = (value: unknown): string => {
  const s = String(value);
  return s.endsWith('s') ? s : `${s}s`;
};

const formatFocalLength = (value: unknown): string => {
  const s = String(value);
  return s.endsWith('mm') ? s : `${s} mm`;
};

const formatLens = (value: unknown): string => String(value).replace(/"/g, '').trim();

/**
 * "NIKON CORPORATION" + "NIKON D850" is one camera, not two words of make
 * followed by the same word again. Most bodies repeat the manufacturer in the
 * model; a few (Leica, Phase One) do not, so the join is conditional.
 */
const formatBody = (make: unknown, model: unknown): string => {
  const mk = make == null ? '' : String(make).trim();
  const md = model == null ? '' : String(model).trim();
  if (!mk) {
    return md;
  }
  if (!md) {
    return mk;
  }
  const lower = md.toLowerCase();
  if (lower.startsWith(mk.toLowerCase())) {
    return md;
  }
  const firstWord = mk.split(/\s+/)[0];
  if (firstWord && lower.startsWith(firstWord.toLowerCase())) {
    return md;
  }
  return `${mk} ${md}`;
};

/** A value that is present and not an empty string. */
const present = (value: unknown): boolean => value !== undefined && value !== null && value !== '';

/** Only the edge the strip hangs from. Deliberately not the height — see below. */
interface Corner {
  left: number;
  top: number;
  width: number;
}

/**
 * Find the analytics band and return its top-left corner.
 *
 * Upstream tags the toggle button with the tooltip it shows, so the button is
 * findable by translating the same key they translate. That is locale-correct
 * by construction, and a semantic hook rather than a match on Tailwind classes
 * which any restyle would silently break.
 *
 * From the button, the band is a fixed two-hops-up-and-one-across walk:
 *
 *   button -> button group -> header row -> next sibling (the band)
 *
 * If that walk fails — upstream restructured the header — fall back to the
 * bottom edge of the header, which is where the band starts anyway. A slightly
 * wrong position beats a crash, and a button that is simply not there yet
 * renders nothing.
 */
function findBand(tooltip: string): { corner: Corner; watch: Element } | null {
  // Compared rather than put in an attribute selector: the translated string is
  // arbitrary text, and a quote in it would break the selector in some locale
  // nobody tests.
  let button: Element | null = null;
  for (const el of Array.from(document.querySelectorAll('[data-tooltip]'))) {
    if (el.getAttribute('data-tooltip') === tooltip) {
      button = el;
      break;
    }
  }
  if (!button) {
    return null;
  }

  const header = button.parentElement?.parentElement;
  if (!header) {
    return null;
  }

  const band = header.nextElementSibling;
  if (band) {
    const rect = band.getBoundingClientRect();
    if (rect.width > 1 && rect.height > 1) {
      return { corner: { left: rect.left, top: rect.top, width: rect.width }, watch: band };
    }
  }

  const headerRect = header.getBoundingClientRect();
  if (headerRect.width < 1) {
    return null;
  }
  return {
    corner: { left: headerRect.left, top: headerRect.bottom, width: headerRect.width },
    watch: header,
  };
}

/**
 * Track the band's top-left corner while it exists.
 *
 * Only the corner is kept, and that is the whole performance story. The band's
 * *height* changes on every frame of the open animation and on every pixel of a
 * resizer drag, while its top edge — the thing this hangs from — does not move
 * at all, because the band grows downwards from a fixed header. Storing the
 * full rect would therefore re-render this on every animation frame to react to
 * a number it never reads. So the expensive cases reduce to real layout
 * changes: the panel widened, the window resized, panels swapped around.
 * Observers for those, and no polling.
 */
function useBandCorner(active: boolean, tooltip: string): Corner | null {
  const [corner, setCorner] = useState<Corner | null>(null);

  useEffect(() => {
    if (!active) {
      setCorner(null);
      return;
    }

    let resizeObserver: ResizeObserver | null = null;
    let watched: Element | null = null;

    const measure = () => {
      const found = findBand(tooltip);
      const next = found?.corner ?? null;

      // Re-point the size observer only when the element itself changed;
      // panels mount and unmount, but re-observing on every measure would make
      // the observer fire again and spin.
      if (found && found.watch !== watched) {
        resizeObserver?.disconnect();
        watched = found.watch;
        resizeObserver?.observe(watched);
      } else if (!found && watched) {
        resizeObserver?.disconnect();
        watched = null;
      }

      setCorner((current) => {
        if (current && next && current.left === next.left && current.top === next.top && current.width === next.width) {
          return current;
        }
        return next;
      });
    };

    resizeObserver = new ResizeObserver(measure);
    const mutationObserver = new MutationObserver(measure);
    mutationObserver.observe(document.body, { childList: true, subtree: true });
    window.addEventListener('resize', measure);
    measure();

    return () => {
      resizeObserver?.disconnect();
      mutationObserver.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [active, tooltip]);

  return corner;
}

interface ChipProps {
  icon: ReactNode;
  label: string;
  value: string;
  className?: string;
}

/**
 * `aria-label` rather than `data-tooltip`, deliberately. The strip is
 * `pointer-events-none` so that it never eats a hover meant for the scope
 * underneath, which also means a tooltip on it could never be triggered — it
 * would be a string nobody can ever read. A label still names the value for a
 * screen reader, where pointer events are irrelevant.
 */
function Chip({ icon, label, value, className }: ChipProps) {
  return (
    <span className={`flex items-center gap-1 min-w-0 ${className ?? ''}`} aria-label={`${label}: ${value}`}>
      <span className="shrink-0 opacity-60" aria-hidden="true">
        {icon}
      </span>
      <span className="truncate">{value}</span>
    </span>
  );
}

export default function CameraSettings() {
  const { t } = useTranslation();
  const agT = useAgTranslation();

  const selectedImage = useEditorStore((s) => s.selectedImage);
  const isWaveformVisible = useEditorStore((s) => s.isWaveformVisible);
  const multiSelectedPaths = useLibraryStore((s) => s.multiSelectedPaths);

  /**
   * Upstream's own test for whether the Adjustments panel is on screen, copied
   * from `MasksPanel.tsx`.
   *
   * This is load-bearing, and not for the reason it looks like. The Masks panel
   * renders a second copy of the whole analytics band, and its toggle button
   * carries the *same English text* — `editor.masks.toggleAnalyticsTooltip` and
   * `editor.adjustments.tooltips.toggleAnalytics` are different keys with the
   * identical value "Toggle Analytics Display". Different keys are therefore no
   * protection at all: with the Adjustments panel hidden, the tooltip lookup
   * would find the Masks button and this would render over the Masks scope.
   *
   * When the Adjustments panel *is* visible, upstream's Masks button switches
   * to `toggleAnalyticsInAdjustments` ("Toggle analytics in Adjustments
   * panel"), which makes the string we look for unique again. So gating on this
   * is what makes the locator unambiguous — and it also enforces the decision
   * that this feature belongs to the Adjustments panel only.
   */
  const isAdjustmentsVisible = useUIStore((s) => {
    const visible = s.uiVisibility;
    const panels = s.activePanels;
    return Boolean(
      (visible?.leftPanel && (panels?.leftTop === Panel.Adjustments || panels?.leftBottom === Panel.Adjustments)) ||
      (visible?.rightPanel && (panels?.rightTop === Panel.Adjustments || panels?.rightBottom === Panel.Adjustments)),
    );
  });

  const tooltip = t('editor.adjustments.tooltips.toggleAnalytics');
  const active = Boolean(isWaveformVisible && isAdjustmentsVisible);
  const corner = useBandCorner(active, tooltip);

  if (!active || !corner) {
    return null;
  }

  const count = multiSelectedPaths?.length ?? 0;

  const box = {
    left: corner.left + BAND_PADDING + INSET,
    top: corner.top + BAND_PADDING + INSET,
    maxWidth: Math.max(0, corner.width - 2 * (BAND_PADDING + INSET)),
  };

  // More than one picture selected: there is no single set of settings to show,
  // and showing the open photo's would be a quiet lie about what is selected.
  if (count > 1) {
    return createPortal(
      <div
        className="fixed z-30 rounded-md bg-black/55 px-2 py-1 text-[11px] text-white/70 backdrop-blur-sm pointer-events-none select-none"
        style={box}
      >
        {agT('analyticsMultiSelection').replace('{count}', String(count))}
      </div>,
      document.body,
    );
  }

  // `isReady` is the gap between opening a photo and its EXIF arriving. Without
  // it the strip blinks empty on every photo change.
  const exif = selectedImage?.isReady ? selectedImage?.exif : null;
  if (!exif) {
    return null;
  }

  const iso = exif.PhotographicSensitivity;
  const aperture = exif.FNumber;
  const shutter = exif.ExposureTime;
  // `FocalLength`, not `FocalLengthIn35mmFilm` as their Metadata panel reads.
  // `extract_metadata` writes the same uncorrected number into both keys, so
  // the 35mm one is wrong on every crop body. Identical today, right later.
  const focal = present(exif.FocalLength) ? exif.FocalLength : exif.FocalLengthIn35mmFilm;
  const body = formatBody(exif.Make, exif.Model);
  const lens = present(exif.LensModel) ? formatLens(exif.LensModel) : '';

  // A photo with no EXIF at all — a scan, an export stripped of metadata. Six
  // dashes over the scope is worse than nothing.
  if (![iso, aperture, shutter, focal].some(present) && !body && !lens) {
    return null;
  }

  return createPortal(
    <div
      className="fixed z-30 flex flex-col gap-0.5 rounded-md bg-black/55 px-2 py-1 text-[11px] leading-tight text-white backdrop-blur-sm pointer-events-none select-none"
      style={box}
    >
      <div className="flex items-center gap-2.5 font-mono">
        <Chip
          icon={<IconIso width={11} height={11} />}
          label={agT('analyticsIso')}
          value={present(iso) ? String(iso) : '—'}
        />
        <Chip
          icon={<IconAperture width={11} height={11} />}
          label={agT('analyticsAperture')}
          value={present(aperture) ? formatAperture(aperture) : '—'}
        />
        <Chip
          icon={<IconShutter width={11} height={11} />}
          label={agT('analyticsShutter')}
          value={present(shutter) ? formatShutter(shutter) : '—'}
        />
        <Chip
          icon={<IconFocalLength width={11} height={11} />}
          label={agT('analyticsFocalLength')}
          value={present(focal) ? formatFocalLength(focal) : '—'}
        />
      </div>
      {(body || lens) && (
        <div className="flex items-center gap-2.5 text-white/60 min-w-0">
          {body && <Chip icon={<Camera size={11} />} label={agT('analyticsCamera')} value={body} className="shrink" />}
          {lens && (
            <Chip
              icon={<IconLens width={11} height={11} />}
              label={agT('analyticsLens')}
              value={lens}
              className="shrink"
            />
          )}
        </div>
      )}
    </div>,
    document.body,
  );
}
