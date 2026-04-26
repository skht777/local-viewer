// PdfCgViewer 用のツールバーラッパー
// - デスクトップ: 自動表示/非表示 (isToolbarVisible で opacity 切替)
// - タッチデバイス: 常時表示 (通常フローに配置)
// - CgToolbar に必要な props を集約して渡す

import type { FitMode, SpreadMode } from "../stores/viewerStore";
import { CgToolbar } from "./CgToolbar";

interface PdfCgViewerToolbarProps {
  isTouch: boolean;
  isToolbarVisible: boolean;
  fitMode: FitMode;
  spreadMode: SpreadMode;
  currentPage: number;
  pageCount: number;
  pdfName: string;
  firstDisplay: number;
  currentEnd: number | undefined;
  onFitWidth: () => void;
  onFitHeight: () => void;
  onCycleSpread: () => void;
  onToggleFullscreen: () => void;
  onGoTo: (index: number) => void;
  onClose: () => void;
  onPrevSet: () => void;
  onNextSet: () => void;
  isSetJumpDisabled: boolean;
}

export function PdfCgViewerToolbar({
  isTouch,
  isToolbarVisible,
  fitMode,
  spreadMode,
  currentPage,
  pageCount,
  pdfName,
  firstDisplay,
  currentEnd,
  onFitWidth,
  onFitHeight,
  onCycleSpread,
  onToggleFullscreen,
  onGoTo,
  onClose,
  onPrevSet,
  onNextSet,
  isSetJumpDisabled,
}: PdfCgViewerToolbarProps) {
  const wrapperClass = isTouch
    ? "relative z-10"
    : `absolute top-0 right-0 left-0 z-10 transition-opacity duration-300 ${
        isToolbarVisible ? "opacity-100" : "pointer-events-none opacity-0"
      }`;
  return (
    <div data-testid="toolbar-wrapper" className={wrapperClass}>
      <CgToolbar
        fitMode={fitMode}
        spreadMode={spreadMode}
        currentIndex={currentPage}
        totalCount={pageCount}
        showSpread={true}
        setName={pdfName}
        currentPage={firstDisplay}
        currentPageEnd={currentEnd}
        onFitWidth={onFitWidth}
        onFitHeight={onFitHeight}
        onCycleSpread={onCycleSpread}
        onToggleFullscreen={onToggleFullscreen}
        onGoTo={onGoTo}
        onClose={onClose}
        onPrevSet={onPrevSet}
        onNextSet={onNextSet}
        isSetJumpDisabled={isSetJumpDisabled}
      />
    </div>
  );
}
