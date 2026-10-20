/**
 * Detail's two tools that open a dialog instead of moving a slider: Denoise…
 * under Noise reduction, and Restore & Enlarge… as Enhance. Ours; they sit
 * beside Sharpening and Noise reduction because they make detail and remove
 * noise, whatever the method (modern-spec, Edit > Detail).
 *
 * The dialogs are the ones the photo's right-click menu opens
 * (useAppContextMenus.ts, Denoise) and the Classic toolbar's sparkles button
 * did (Restore & Enlarge, deleted with that toolbar). Both act on the open
 * photo only, so in the grid's read-only view they are greyed.
 */

import AdjustmentSubSection from '../components/adjustments/AdjustmentSubSection';
import { useEditorStore } from '../store/useEditorStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';
import { useAgTranslation } from './locales';
import { openSuperResolution } from './superResolution';

/** Their Denoise dialog for the open photo, as the right-click menu opens it. */
export function DenoiseButton() {
  const t = useAgTranslation();
  const photo = useEditorStore((s) => s.selectedImage);
  const open = () => {
    if (!photo) return;
    useUIStore.getState().setUI({
      denoiseModalState: {
        isOpen: true,
        isProcessing: false,
        previewBase64: null,
        error: null,
        targetPaths: [photo.path],
        progressMessage: null,
        isRaw: photo.isRaw || false,
      },
    });
  };
  return (
    <button className="agm-btn agm-btn-box w-full mt-1" disabled={!photo?.isReady} onClick={open}>
      {t('detailDenoise')}
    </button>
  );
}

/**
 * Restore & Enlarge, after Detail's other tools. Enlarging is AI, so in
 * AI-Free mode it is not shown, as the toolbar button was not.
 */
export function EnhanceSection({ order }: { order: number }) {
  const t = useAgTranslation();
  const photo = useEditorStore((s) => s.selectedImage);
  const aiFree = useSettingsStore((s) => s.appSettings?.aiProvider === 'ai-free');
  if (aiFree) return null;
  return (
    <AdjustmentSubSection id="agEnhance" order={order} title={t('detailEnhance')}>
      <button
        className="agm-btn agm-btn-box w-full mt-1"
        disabled={!photo?.isReady}
        onClick={() => photo && openSuperResolution([photo.path])}
      >
        {t('detailRestore')}
      </button>
    </AdjustmentSubSection>
  );
}
