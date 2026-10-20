import { useShallow } from 'zustand/react/shallow';

import CommunityPage from '../panel/CommunityPage';
import MainLibrary from '../panel/MainLibrary';

import { useUIStore } from '../../store/useUIStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useProcessStore } from '../../store/useProcessStore';
import { useSettingsStore } from '../../store/useSettingsStore';

import { ImageFile, LibraryViewMode, ThumbnailAspectRatio, ThumbnailSize } from '../ui/AppProperties';
import { GroupBadgeInfo, GroupId } from '../../utils/imageGrouping';

interface LibraryViewProps {
  sortedImageList: ImageFile[];
  groupBadgeInfo: Map<GroupId, GroupBadgeInfo> | null;
  thumbnailSize: ThumbnailSize;
  thumbnailAspectRatio: ThumbnailAspectRatio;
  libraryViewMode: LibraryViewMode;
  isAndroid: boolean;
  setThumbnailSize: (size: ThumbnailSize) => void;
  setThumbnailAspectRatio: (ratio: ThumbnailAspectRatio) => void;
  setLibraryViewMode: (mode: LibraryViewMode) => void;
  handleClearSelection: () => void;
  handleLibraryImageSingleClick: (...args: any) => void;
  handleImageSelect: (...args: any) => void;
  handleThumbnailContextMenu: (...args: any) => void;
  handleMainLibraryContextMenu: (...args: any) => void;
  handleContinueSession: (...args: any) => void;
  handleGoHome: (...args: any) => void;
  handleOpenFolder: (...args: any) => void;
  handleImportClick: (path: string) => void;
  handleLibraryRefresh: () => Promise<void>;
  requestThumbnails: any;
}

export default function LibraryView({
  sortedImageList,
  groupBadgeInfo,
  thumbnailSize,
  thumbnailAspectRatio,
  libraryViewMode,
  isAndroid,
  setThumbnailSize,
  setThumbnailAspectRatio,
  setLibraryViewMode,
  handleClearSelection,
  handleLibraryImageSingleClick,
  handleImageSelect,
  handleThumbnailContextMenu,
  handleMainLibraryContextMenu,
  handleContinueSession,
  handleGoHome,
  handleOpenFolder,
  handleImportClick,
  handleLibraryRefresh,
  requestThumbnails,
}: LibraryViewProps) {
  const { activeView, setUI } = useUIStore(
    useShallow((state) => ({
      activeView: state.activeView,
      setUI: state.setUI,
    })),
  );

  const {
    rootPaths,
    currentFolderPath,
    libraryActivePath,
    multiSelectedPaths,
    imageRatings,
    isViewLoading,
    isTreeLoading,
  } = useLibraryStore(
    useShallow((state) => ({
      rootPaths: state.rootPaths,
      currentFolderPath: state.currentFolderPath,
      libraryActivePath: state.libraryActivePath,
      multiSelectedPaths: state.multiSelectedPaths,
      imageRatings: state.imageRatings,
      isViewLoading: state.isViewLoading,
      isTreeLoading: state.isTreeLoading,
    })),
  );

  const { appSettings, supportedTypes, theme, handleSettingsChange } = useSettingsStore(
    useShallow((state) => ({
      appSettings: state.appSettings,
      supportedTypes: state.supportedTypes,
      theme: state.theme,
      handleSettingsChange: state.handleSettingsChange,
    })),
  );

  const { aiModelDownloadStatus, importState, indexingProgress, isIndexing, thumbnailProgress } = useProcessStore(
    useShallow((state) => ({
      aiModelDownloadStatus: state.aiModelDownloadStatus,
      importState: state.importState,
      indexingProgress: state.indexingProgress,
      isIndexing: state.isIndexing,
      thumbnailProgress: state.thumbnailProgress,
    })),
  );

  return (
    <div className="flex flex-row grow h-full min-h-0">
      <div className="flex-1 flex flex-col min-w-0 gap-2">
        {activeView === 'community' ? (
          <CommunityPage
            onBackToLibrary={() => setUI({ activeView: 'library' })}
            supportedTypes={supportedTypes}
            imageList={sortedImageList}
            currentFolderPath={currentFolderPath}
          />
        ) : (
          <MainLibrary
            modern
            activePath={libraryActivePath}
            aiModelDownloadStatus={aiModelDownloadStatus}
            appSettings={appSettings}
            currentFolderPath={currentFolderPath}
            groupBadgeInfo={groupBadgeInfo}
            imageList={sortedImageList}
            imageRatings={imageRatings}
            importState={importState}
            indexingProgress={indexingProgress}
            isIndexing={isIndexing}
            isLoading={isViewLoading}
            isTreeLoading={isTreeLoading}
            isAndroid={isAndroid}
            libraryViewMode={libraryViewMode}
            multiSelectedPaths={multiSelectedPaths}
            onClearSelection={handleClearSelection}
            onContextMenu={handleThumbnailContextMenu}
            onContinueSession={handleContinueSession}
            onEmptyAreaContextMenu={handleMainLibraryContextMenu}
            onGoHome={handleGoHome}
            onImageClick={handleLibraryImageSingleClick}
            onImageDoubleClick={handleImageSelect}
            onImportClick={() => handleImportClick(currentFolderPath as string)}
            onLibraryRefresh={handleLibraryRefresh}
            onOpenFolder={handleOpenFolder}
            onSettingsChange={handleSettingsChange}
            onThumbnailAspectRatioChange={setThumbnailAspectRatio}
            onThumbnailSizeChange={setThumbnailSize}
            onRequestThumbnails={requestThumbnails}
            rootPaths={rootPaths}
            setLibraryViewMode={setLibraryViewMode}
            theme={theme}
            thumbnailAspectRatio={thumbnailAspectRatio}
            thumbnailProgress={thumbnailProgress}
            thumbnailSize={thumbnailSize}
            onNavigateToCommunity={() => setUI({ activeView: 'community' })}
          />
        )}
      </div>
    </div>
  );
}
