// PDF CG モードビューワー: 1ページ or 見開き表示
// - usePdfDocument で PDF 読み込み
// - PdfCanvas で canvas 描画
// - CgToolbar (showSpread=true), キーボード等を再利用
// - spreadMode に応じた 1 ページ / 2 ページ横並び表示
// - ResizeObserver でコンテナサイズを動的計測
// - useSetJump: currentNodeId = pdfNodeId (PDF 自身)

import { useCallback, useState } from "react";
import type { AncestorEntry } from "../types/api";
import type { SortOrder, ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { useClickToTurnPage } from "../hooks/useClickToTurnPage";
import { useCursorAutoHide } from "../hooks/useCursorAutoHide";
import { useFullscreen } from "../hooks/useFullscreen";
import { useCgNavigation } from "../hooks/useCgNavigation";
import { useCgKeyboard } from "../hooks/useCgKeyboard";
import { useSetJump } from "../hooks/useSetJump";
import { useSiblingPrefetch } from "../hooks/useSiblingPrefetch";
import { useToast } from "../hooks/useToast";
import { useToolbarAutoHide } from "../hooks/useToolbarAutoHide";
import { usePdfContainerSize } from "../hooks/usePdfContainerSize";
import { usePdfDocument } from "../hooks/usePdfDocument";
import { usePdfPageState } from "../hooks/usePdfPageState";
import { usePdfRenderCache } from "../hooks/usePdfRenderCache";
import { useViewerBoundaryNavigation } from "../hooks/useViewerBoundaryNavigation";
import { formatPageLabel } from "../utils/formatPageLabel";
import { PdfCanvas } from "./PdfCanvas";
import { PageSlider } from "./PageSlider";
import { PdfCgViewerOverlays } from "./PdfCgViewerOverlays";
import { PdfCgViewerToolbar } from "./PdfCgViewerToolbar";
import { renderPdfStatus } from "./PdfViewerStatus";

interface PdfCgViewerProps {
  pdfNodeId: string;
  pdfName: string;
  parentNodeId: string | null;
  ancestors?: AncestorEntry[];
  initialPage: number;
  mode: ViewerMode;
  sort?: SortOrder;
  onPageChange: (page: number) => void;
  onClose: () => void;
}

// 見開き表示時の派生値: 各ページの containerWidth と 1-based ページラベル
function getPdfDisplayRange(
  displayIndices: number[],
  containerWidth: number,
): {
  pageContainerWidth: number;
  firstDisplay: number;
  currentEnd: number | undefined;
} {
  const pageContainerWidth = displayIndices.length > 1 ? containerWidth / 2 : containerWidth;
  const first = displayIndices.length > 0 ? displayIndices[0] + 1 : 1;
  const last = displayIndices.length > 0 ? displayIndices[displayIndices.length - 1] + 1 : 1;
  return {
    pageContainerWidth,
    firstDisplay: first,
    currentEnd: displayIndices.length > 1 ? last : undefined,
  };
}

export function PdfCgViewer({
  pdfNodeId,
  pdfName,
  parentNodeId,
  ancestors,
  initialPage,
  mode,
  sort,
  onPageChange,
  onClose,
}: PdfCgViewerProps) {
  const fitMode = useViewerStore((s) => s.fitMode);
  const spreadMode = useViewerStore((s) => s.spreadMode);
  const setFitMode = useViewerStore((s) => s.setFitMode);
  const cycleSpreadMode = useViewerStore((s) => s.cycleSpreadMode);
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const { toggleFullscreen } = useFullscreen();

  const { document, pageCount, isLoading, error } = usePdfDocument(`/api/file/${pdfNodeId}`);
  // 描画キャッシュ (PdfCgViewer のみ適用 — PdfMangaViewer には渡さない)
  const renderCache = usePdfRenderCache();
  const { currentPage, handlePageChange } = usePdfPageState(initialPage, onPageChange);
  const nav = useCgNavigation(pageCount, currentPage, handlePageChange, spreadMode);

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();
  const { toastMessage, toastDuration, showToast, dismissToast } = useToast();
  const { handleGoNext, handleGoPrev } = useViewerBoundaryNavigation({
    nav,
    showToast,
    firstMessage: "最初のページです",
    lastMessage: "最後のページです",
  });

  const [isHelpOpen, setIsHelpOpen] = useState(false);

  const setJump = useSetJump({
    currentNodeId: pdfNodeId,
    parentNodeId,
    ancestors,
    mode,
    sort,
    onBoundary: showToast,
  });
  useSiblingPrefetch({ currentNodeId: pdfNodeId, parentNodeId, ancestors, sort });

  // Escape: ダイアログ閉じのみ（ビューワー閉じは B キー）
  const handleEscape = useCallback(() => {
    if (isHelpOpen) {
      setIsHelpOpen(false);
      return;
    }
    if (setJump.prompt) {
      setJump.dismissPrompt();
    }
  }, [isHelpOpen, setJump]);

  const { containerSize, imageAreaRef, combinedRef } = usePdfContainerSize();
  const { resetCursorTimer } = useCursorAutoHide(imageAreaRef);
  const handleClick = useClickToTurnPage(handleGoNext, handleGoPrev);

  useCgKeyboard({
    goNext: handleGoNext,
    goPrev: handleGoPrev,
    goFirst: nav.goFirst,
    goLast: nav.goLast,
    onEscape: handleEscape,
    onClose,
    toggleFullscreen,
    setFitWidth: () => setFitMode("width"),
    setFitHeight: () => setFitMode("height"),
    cycleSpread: cycleSpreadMode,
    scrollUp: () => imageAreaRef.current?.scrollBy({ top: -100, behavior: "instant" }),
    scrollDown: () => imageAreaRef.current?.scrollBy({ top: 100, behavior: "instant" }),
    goNextSet: setJump.prompt ? undefined : setJump.goNextSet,
    goPrevSet: setJump.prompt ? undefined : setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
    toggleHelp: () => setIsHelpOpen((prev) => !prev),
    // タイトル + ページ番号を 3 秒トースト表示（見開き時は "3-4 / 12" 形式）
    showTitle: () => {
      const indices = nav.displayIndices;
      if (indices.length === 0) {
        return;
      }
      const first = indices[0] + 1;
      const last = indices[indices.length - 1] + 1;
      const end = indices.length > 1 ? last : undefined;
      showToast(formatPageLabel(pdfName, first, pageCount, end), 3000);
    },
  });

  // 見開き表示用の派生値 (containerWidth / 1-based ページラベル)
  const { displayIndices } = nav;
  const display = getPdfDisplayRange(displayIndices, containerSize.width);

  const status = renderPdfStatus({ isLoading, error, document, onClose });
  if (status.shouldEarlyReturn || !document) {
    return status.element;
  }

  return (
    <div data-testid="pdf-cg-viewer" className="fixed inset-0 z-50 flex bg-black">
      <div ref={containerCallbackRef} className="relative flex flex-1 flex-col overflow-hidden">
        <PdfCgViewerToolbar
          isTouch={isTouch}
          isToolbarVisible={isToolbarVisible}
          fitMode={fitMode}
          spreadMode={spreadMode}
          currentPage={currentPage}
          pageCount={pageCount}
          pdfName={pdfName}
          firstDisplay={display.firstDisplay}
          currentEnd={display.currentEnd}
          onFitWidth={() => setFitMode("width")}
          onFitHeight={() => setFitMode("height")}
          onCycleSpread={cycleSpreadMode}
          onToggleFullscreen={toggleFullscreen}
          onGoTo={nav.goTo}
          onClose={onClose}
          onPrevSet={setJump.goPrevSet}
          onNextSet={setJump.goNextSet}
          isSetJumpDisabled={setJump.prompt != null || viewerTransitionId > 0}
        />
        <div
          ref={combinedRef}
          data-testid="pdf-cg-page-area"
          className="flex flex-1 items-center justify-center overflow-auto"
          onClick={handleClick}
          onMouseMove={resetCursorTimer}
        >
          {displayIndices.map((pageIdx) => (
            <div
              key={pageIdx}
              className={
                displayIndices.length > 1
                  ? "flex min-w-0 flex-1 items-center justify-center"
                  : "flex items-center justify-center"
              }
            >
              <PdfCanvas
                document={document}
                pageNumber={pageIdx + 1}
                fitMode={fitMode}
                containerWidth={display.pageContainerWidth}
                renderCache={renderCache}
                enableTextLayer={true}
                containerHeight={containerSize.height}
              />
            </div>
          ))}
        </div>
        <PageSlider
          currentIndex={currentPage}
          totalCount={pageCount}
          onGoTo={nav.goTo}
          containerRef={imageAreaRef}
          onSliderActivity={resetCursorTimer}
        />
        <PdfCgViewerOverlays
          toastMessage={toastMessage}
          toastDuration={toastDuration}
          onToastDismiss={dismissToast}
          isHelpOpen={isHelpOpen}
          onHelpClose={() => setIsHelpOpen(false)}
          prompt={setJump.prompt}
        />
      </div>
    </div>
  );
}
