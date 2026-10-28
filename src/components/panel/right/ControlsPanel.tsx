import React, { useCallback } from 'react';
import {
  RotateCcw,
  Copy,
  ClipboardPaste,
  PencilSparkles,
  LayoutList,
  MoreHorizontal,
  Check,
  RefreshCw,
  SlidersHorizontal,
} from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import BasicAdjustments from '../../adjustments/Basic';
import CurveGraph from '../../adjustments/Curves';
import ColorPanel from '../../adjustments/Color';
import DetailsPanel from '../../adjustments/Details';
import EffectsPanel from '../../adjustments/Effects';
import CollapsibleSection from '../../ui/CollapsibleSection';
import AdjustmentSectionsSubMenu from './AdjustmentSectionsSubMenu';
import { Adjustments, INITIAL_ADJUSTMENTS, getVisibleAdjustmentSections } from '../../../utils/adjustments';
import {
  EDIT_SECTION_KEYS,
  initialValues,
  PROFILE_KEYS,
  SECTION_EYES_OPEN,
  type SectionVisibility,
} from '../../../argentum/editSections';
import ProfileSection, { PROFILE_SECTION } from '../../../argentum/ProfileSection';
import { useAgTranslation } from '../../../argentum/locales';
import { useContextMenu } from '../../../context/ContextMenuContext';
import { OPTION_SEPARATOR } from '../../ui/AppProperties';
import Text from '../../ui/Text';
import { TextVariants, TextColors, TextWeights } from '../../../types/typography';
import { useShallow } from 'zustand/react/shallow';
import { useEditorStore } from '../../../store/useEditorStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { useUIStore } from '../../../store/useUIStore';
import { useEditorActions } from '../../../hooks/useEditorActions';

/**
 * Argentum: the Modern window's grid shows the selected photo's settings here,
 * read-only, without opening it: its values come in `readOnly`, nothing can be
 * changed, and Enter opens the photo to edit it.
 */
export interface ReadOnlyAdjustments {
  adjustments: Adjustments;
}

export default function Controls({ readOnly }: { readOnly?: ReadOnlyAdjustments }) {
  const { t } = useTranslation();
  const { showContextMenu } = useContextMenu();
  const {
    setAdjustments: setLiveAdjustments,
    handleAutoAdjustments,
    handleCopyAdjustments,
    handlePasteAdjustments,
    handleLutSelect,
    setLutPreviewOverride,
  } = useEditorActions();
  const setAdjustments = readOnly ? () => {} : setLiveAdjustments;

  const agT = useAgTranslation();
  const { appSettings, theme, handleSettingsChange } = useSettingsStore(
    useShallow((state) => ({
      appSettings: state.appSettings,
      theme: state.theme,
      handleSettingsChange: state.handleSettingsChange,
    })),
  );

  const visibleSections = getVisibleAdjustmentSections(appSettings?.adjustmentLayout);

  const { collapsibleSectionsState, setUI } = useUIStore(
    useShallow((state) => ({
      collapsibleSectionsState: state.collapsibleSectionsState,
      setUI: state.setUI,
    })),
  );

  const { liveAdjustments, copiedSectionAdjustments, histogram, selectedImage, isWbPickerActive, setEditor } =
    useEditorStore(
      useShallow((state) => ({
        liveAdjustments: state.adjustments,
        copiedSectionAdjustments: state.copiedSectionAdjustments,
        histogram: state.histogram,
        selectedImage: state.selectedImage,
        isWbPickerActive: state.isWbPickerActive,
        setEditor: state.setEditor,
      })),
    );

  const adjustments = readOnly?.adjustments ?? liveAdjustments;

  const setCopiedSectionAdjustments = useCallback(
    (val: any) => setEditor({ copiedSectionAdjustments: val }),
    [setEditor],
  );

  const toggleWbPicker = useCallback(
    () => setEditor((state) => ({ isWbPickerActive: !state.isWbPickerActive })),
    [setEditor],
  );

  const onDragStateChange = useCallback(
    (isDragging: boolean) => setEditor({ isSliderDragging: isDragging }),
    [setEditor],
  );

  const setCollapsibleState = useCallback(
    (updater: any) =>
      setUI((state) => ({
        collapsibleSectionsState: typeof updater === 'function' ? updater(state.collapsibleSectionsState) : updater,
      })),
    [setUI],
  );

  const handleToggleVisibility = (sectionName: string) => {
    setAdjustments((prev: Adjustments) => {
      const currentVisibility: SectionVisibility = prev.sectionVisibility || SECTION_EYES_OPEN;
      return {
        ...prev,
        sectionVisibility: {
          ...currentVisibility,
          [sectionName]: !currentVisibility[sectionName],
        },
      };
    });
  };

  // Argentum: every section as the panel arranges them, Profile included (argentum/editSections.ts).
  const handleResetAdjustments = () => {
    setAdjustments((prev: Adjustments) => ({
      ...prev,
      ...initialValues([...Object.values(EDIT_SECTION_KEYS).flat(), ...PROFILE_KEYS]),
      sectionVisibility: { ...SECTION_EYES_OPEN },
    }));
  };

  const handleToggleSection = (section: string) => {
    setCollapsibleState((prev: any) => {
      const isOpening = !prev[section];
      if (appSettings?.enableFocusMode && isOpening) {
        const newState = { ...prev };
        Object.keys(newState).forEach((key) => {
          newState[key] = false;
        });
        newState[section] = true;
        return newState;
      }
      return { ...prev, [section]: !prev[section] };
    });
  };

  // Argentum: the section menu's Reset, also the reset button in a section's
  // header in the Modern look.
  const resetSection = (sectionName: string) => {
    const sectionKeys = EDIT_SECTION_KEYS[sectionName];
    if (!sectionKeys) {
      return;
    }
    setAdjustments((prev: Adjustments) => ({
      ...prev,
      ...initialValues(sectionKeys),
      sectionVisibility: {
        ...(prev.sectionVisibility || SECTION_EYES_OPEN),
        [sectionName]: true,
      },
    }));
  };

  // Argentum: the header's ⋯, what acts on the whole photo (modern-spec, Edit header).
  const moreMenu = (event: React.MouseEvent<HTMLButtonElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const sync = appSettings?.copyPasteSettings;
    showContextMenu(rect.left, rect.bottom, [
      { label: t('ui.bottomBar.tooltips.copySettings'), icon: Copy, onClick: () => handleCopyAdjustments() },
      {
        label: t('ui.bottomBar.tooltips.pasteSettings'),
        icon: ClipboardPaste,
        onClick: () => handlePasteAdjustments(),
      },
      {
        label: `${t('modals.copyPaste.title')}…`,
        icon: SlidersHorizontal,
        onClick: () => setUI({ isCopyPasteSettingsModalOpen: true }),
      },
      { type: OPTION_SEPARATOR },
      {
        label: t('modals.copyPaste.autoSyncTitle'),
        // A tick while it is on.
        icon: sync?.autoSync ? Check : RefreshCw,
        disabled: !appSettings || !sync,
        onClick: () =>
          appSettings &&
          sync &&
          handleSettingsChange({ ...appSettings, copyPasteSettings: { ...sync, autoSync: !sync.autoSync } }),
      },
      { type: OPTION_SEPARATOR },
      {
        label: t('editor.adjustments.actions.customizePanels'),
        icon: LayoutList,
        submenu: [{ customComponent: AdjustmentSectionsSubMenu }],
      },
    ]);
  };

  const handleSectionContextMenu = (event: any, sectionName: string) => {
    event.preventDefault();
    event.stopPropagation();

    const sectionKeys = EDIT_SECTION_KEYS[sectionName];
    if (!sectionKeys) {
      return;
    }

    const handleCopy = () => {
      const adjustmentsToCopy: any = {};
      for (const key of sectionKeys) {
        if (Object.prototype.hasOwnProperty.call(adjustments, key)) {
          adjustmentsToCopy[key] = JSON.parse(JSON.stringify(adjustments[key as keyof Adjustments]));
        }
      }
      setCopiedSectionAdjustments({ section: sectionName, values: adjustmentsToCopy });
    };

    const handlePaste = () => {
      if (!copiedSectionAdjustments || copiedSectionAdjustments.section !== sectionName) {
        return;
      }
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        ...copiedSectionAdjustments.values,
        sectionVisibility: {
          ...(prev.sectionVisibility || SECTION_EYES_OPEN),
          [sectionName]: true,
        },
      }));
    };

    const isPasteAllowed = copiedSectionAdjustments && copiedSectionAdjustments.section === sectionName;
    const translatedSection = t(`editor.adjustments.sections.${sectionName}`);

    const pasteLabel = copiedSectionAdjustments
      ? t('editor.adjustments.actions.pasteLabel', { section: translatedSection })
      : t('editor.adjustments.actions.pasteSettings');

    const options: any = [
      {
        label: t('editor.adjustments.actions.copySectionSettings', { section: translatedSection }),
        icon: Copy,
        onClick: handleCopy,
      },
      { label: pasteLabel, icon: ClipboardPaste, onClick: handlePaste, disabled: !isPasteAllowed },
      { type: OPTION_SEPARATOR },
      {
        label: t('editor.adjustments.actions.resetSectionSettings', { section: translatedSection }),
        icon: RotateCcw,
        onClick: () => resetSection(sectionName),
      },
      { type: OPTION_SEPARATOR },
      {
        label: t('editor.adjustments.actions.customizePanels'),
        icon: LayoutList,
        submenu: [{ customComponent: AdjustmentSectionsSubMenu }],
      },
    ];

    showContextMenu(event.clientX, event.clientY, options);
  };

  return (
    // Argentum: data-ag-adjustments is what the Modern look (modern/modern.css) restyles.
    <div className="flex flex-col h-full" data-ag-adjustments>
      <div className="p-3 flex justify-between items-center shrink-0 border-b border-surface">
        <Text variant={TextVariants.title}>{t('editor.adjustments.title')}</Text>
        <div className="flex items-center gap-1">
          <button
            className="p-2 rounded-full hover:bg-surface disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            disabled={!selectedImage}
            onClick={handleAutoAdjustments}
            data-tooltip={t('editor.adjustments.tooltips.autoAdjust')}
          >
            <PencilSparkles size={18} />
          </button>
          <button
            className="p-2 rounded-full hover:bg-surface disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            disabled={!selectedImage}
            onClick={handleResetAdjustments}
            data-tooltip={t('editor.adjustments.tooltips.resetAdjustments')}
          >
            <RotateCcw size={18} />
          </button>
          <button
            className="p-2 rounded-full hover:bg-surface disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            disabled={!selectedImage}
            onClick={moreMenu}
            data-tooltip={agT('editMoreTip')}
          >
            <MoreHorizontal size={18} />
          </button>
        </div>
      </div>

      <div
        className={clsx('grow overflow-y-scroll p-3 flex flex-col gap-2', readOnly && 'agm-readonly')}
        inert={!!readOnly}
      >
        {/* Argentum: Profile first, ahead of the sections the Customise menu orders. */}
        {selectedImage && !readOnly && (
          <ProfileSection
            adjustments={adjustments}
            setAdjustments={setAdjustments}
            tonemapperOverride={!!appSettings?.tonemapperOverrideEnabled}
            onToggle={() => handleToggleSection(PROFILE_SECTION)}
          />
        )}
        {selectedImage || readOnly ? (
          visibleSections.map((sectionName: string) => {
            const SectionComponent: any = {
              basic: BasicAdjustments,
              curves: CurveGraph,
              color: ColorPanel,
              details: DetailsPanel,
              effects: EffectsPanel,
            }[sectionName];

            const title = t(`editor.adjustments.sections.${sectionName}`);
            const sectionVisibility = adjustments.sectionVisibility || SECTION_EYES_OPEN;

            return (
              <div className="shrink-0 group relative" key={sectionName}>
                <CollapsibleSection
                  isContentVisible={sectionVisibility[sectionName as keyof SectionVisibility]}
                  isOpen={collapsibleSectionsState[sectionName as keyof typeof collapsibleSectionsState]}
                  onContextMenu={(e: any) => handleSectionContextMenu(e, sectionName)}
                  onToggle={() => handleToggleSection(sectionName)}
                  onToggleVisibility={() => handleToggleVisibility(sectionName)}
                  title={title}
                >
                  <SectionComponent
                    adjustments={adjustments}
                    setAdjustments={setAdjustments}
                    histogram={histogram}
                    theme={theme}
                    handleLutSelect={handleLutSelect}
                    onLutHover={setLutPreviewOverride}
                    appSettings={appSettings}
                    asShotWhiteBalance={readOnly ? undefined : selectedImage?.asShotWhiteBalance}
                    isWbPickerActive={isWbPickerActive}
                    toggleWbPicker={toggleWbPicker}
                    onDragStateChange={onDragStateChange}
                  />
                </CollapsibleSection>
                {/* Argentum: over the header's right end, shown only in the Modern look (modernLook.css). */}
                <button
                  className="ag-section-reset"
                  onClick={() => resetSection(sectionName)}
                  data-tooltip={t('editor.adjustments.actions.resetSectionSettings', { section: title })}
                  aria-label={t('editor.adjustments.actions.resetSectionSettings', { section: title })}
                >
                  <RotateCcw size={13} />
                </button>
              </div>
            );
          })
        ) : (
          <div className="flex items-center justify-center h-full">
            <Text
              variant={TextVariants.heading}
              color={TextColors.secondary}
              weight={TextWeights.normal}
              className="text-center"
            >
              {t('editor.ai.noImageSelected')}
            </Text>
          </div>
        )}
      </div>
    </div>
  );
}
