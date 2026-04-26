// PDF マンガモードビューワー: 全ページを仮想スクロールで表示
// - usePdfPageSizes で estimateSize の精度を保証
// - PdfCanvas で各ページを canvas 描画 (renderCache は CG モード専用のためここでは渡さない)
// - MangaToolbar, PageCounter, キーボード等を再利用

import { useCallback, useRef, useState } from "react";
import type { AncestorEntry } from "../types/api";
import type { SortOrder, ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { useCursorAutoHide } from "../hooks/useCursorAutoHide";
import { useFullscreen } from "../hooks/useFullscreen";
import { useMangaScroll } from "../hooks/useMangaScroll";
import { useMangaKeyboard } from "../hooks/useMangaKeyboard";
import { useMangaVirtualizer } from "../hooks/useMangaVirtualizer";
import { useSetJump } from "../hooks/useSetJump";
import { useSiblingPrefetch } from "../hooks/useSiblingPrefetch";
import { useToast } from "../hooks/useToast";
import { useToolbarAutoHide } from "../hooks/useToolbarAutoHide";
import { usePdfDocument } from "../hooks/usePdfDocument";
import { usePdfPageSizes } from "../hooks/usePdfPageSizes";
import { useUrlIndexSync } from "../hooks/useUrlIndexSync";
import { formatPageLabel } from "../utils/formatPageLabel";
import { PdfCanvas } from "./PdfCanvas";
import { PdfMangaViewerOverlays } from "./PdfMangaViewerOverlays";
import { PdfMangaViewerToolbar } from "./PdfMangaViewerToolbar";
import { renderPdfStatus } from "./PdfViewerStatus";
import { VerticalPageSlider } from "./VerticalPageSlider";

interface PdfMangaViewerProps {
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

export function PdfMangaViewer({
  pdfNodeId,
  pdfName,
  parentNodeId,
  ancestors,
  initialPage,
  mode,
  sort,
  onPageChange,
  onClose,
}: PdfMangaViewerProps) {
  const zoomLevel = useViewerStore((s) => s.zoomLevel);
  const setZoomLevel = useViewerStore((s) => s.setZoomLevel);
  const zoomIn = useViewerStore((s) => s.zoomIn);
  const zoomOut = useViewerStore((s) => s.zoomOut);
  const scrollSpeed = useViewerStore((s) => s.scrollSpeed);
  const setScrollSpeed = useViewerStore((s) => s.setScrollSpeed);
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const { toggleFullscreen } = useFullscreen();

  const { document, pageCount, isLoading, error } = usePdfDocument(`/api/file/${pdfNodeId}`);
  const { pageSizes, isReady: pageSizesReady } = usePdfPageSizes(document);

  const anchorIndexRef = useRef(0);
  const { virtualizer, scrollRef, scrollElement } = useMangaVirtualizer({
    count: pageCount,
    zoomLevel,
    initialIndex: initialPage - 1,
    pageSizes,
    pageSizesReady,
    anchorIndexRef,
  });

  const mangaScroll = useMangaScroll({
    virtualizer,
    scrollElement,
    totalCount: pageCount,
    scrollSpeed,
  });

  // zoom anchor 用に最新 index を ref に反映
  anchorIndexRef.current = mangaScroll.currentIndex;

  // currentIndex を URL に即時同期 (1-based に変換)
  const handleIndexChange = useCallback(
    (idx: number) => {
      onPageChange(idx + 1);
    },
    [onPageChange],
  );
  useUrlIndexSync({
    currentIndex: mangaScroll.currentIndex,
    externalIndex: initialPage - 1,
    onChange: handleIndexChange,
    debounceMs: null,
  });

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();
  const [isHelpOpen, setIsHelpOpen] = useState(false);
  const { toastMessage, toastDuration, showToast, dismissToast } = useToast();

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

  useMangaKeyboard({
    scrollUp: mangaScroll.scrollUp,
    scrollDown: mangaScroll.scrollDown,
    scrollToTop: mangaScroll.scrollToTop,
    scrollToBottom: mangaScroll.scrollToBottom,
    onEscape: handleEscape,
    onClose,
    toggleFullscreen,
    goNextSet: setJump.prompt ? undefined : setJump.goNextSet,
    goPrevSet: setJump.prompt ? undefined : setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
    zoomIn,
    zoomOut,
    zoomReset: () => setZoomLevel(100),
    toggleHelp: () => setIsHelpOpen((prev) => !prev),
    showTitle: () =>
      showToast(formatPageLabel(pdfName, mangaScroll.currentIndex + 1, pageCount), 3000),
  });

  const { resetCursorTimer } = useCursorAutoHide(scrollRef);

  const status = renderPdfStatus({ isLoading, error, document, onClose });
  if (status.shouldEarlyReturn || !document) {
    return status.element;
  }

  return (
    <div data-testid="pdf-manga-viewer" className="fixed inset-0 z-50 flex bg-black">
      <div ref={containerCallbackRef} className="relative flex flex-1 flex-col overflow-hidden">
        <PdfMangaViewerToolbar
          isTouch={isTouch}
          isToolbarVisible={isToolbarVisible}
          currentIndex={mangaScroll.currentIndex}
          totalCount={pageCount}
          zoomLevel={zoomLevel}
          scrollSpeed={scrollSpeed}
          pdfName={pdfName}
          onScrollToImage={mangaScroll.scrollToImage}
          onZoomIn={zoomIn}
          onZoomOut={zoomOut}
          onZoomChange={setZoomLevel}
          onScrollSpeedChange={setScrollSpeed}
          onToggleFullscreen={toggleFullscreen}
          onClose={onClose}
          onPrevSet={setJump.goPrevSet}
          onNextSet={setJump.goNextSet}
          isSetJumpDisabled={setJump.prompt != null || viewerTransitionId > 0}
        />
        <div
          ref={scrollRef}
          data-testid="pdf-manga-scroll-area"
          className="flex-1 overflow-auto"
          onMouseMove={resetCursorTimer}
        >
          <div
            className="relative mx-auto"
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: `${zoomLevel}%`,
            }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => (
              <div
                key={virtualRow.index}
                ref={virtualizer.measureElement}
                data-index={virtualRow.index}
                className="absolute left-0 w-full"
                style={{ top: `${virtualRow.start}px` }}
              >
                <PdfCanvas
                  document={document}
                  pageNumber={virtualRow.index + 1}
                  fitMode="width"
                  containerWidth={scrollRef.current?.clientWidth ?? 800}
                  containerHeight={scrollRef.current?.clientHeight ?? 600}
                  enableTextLayer={true}
                />
              </div>
            ))}
          </div>
        </div>
        <VerticalPageSlider
          currentIndex={mangaScroll.currentIndex}
          totalCount={pageCount}
          onGoTo={mangaScroll.scrollToImage}
          containerRef={scrollRef}
          onSliderActivity={resetCursorTimer}
        />
        <PdfMangaViewerOverlays
          isHelpOpen={isHelpOpen}
          onHelpClose={() => setIsHelpOpen(false)}
          toastMessage={toastMessage}
          toastDuration={toastDuration}
          onToastDismiss={dismissToast}
          prompt={setJump.prompt}
        />
      </div>
    </div>
  );
}
