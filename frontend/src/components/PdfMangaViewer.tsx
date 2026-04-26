// PDF マンガモードビューワー: 全ページを仮想スクロールで表示
// - usePdfPageSizes で estimateSize の精度を保証
// - PdfCanvas で各ページを canvas 描画
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
import { KeyboardHelp, MANGA_SHORTCUTS } from "./KeyboardHelp";
import { MangaToolbar } from "./MangaToolbar";
import { NavigationPrompt } from "./NavigationPrompt";
import { Toast } from "./Toast";
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

  // PDF ドキュメント読み込み
  const { document, pageCount, isLoading, error } = usePdfDocument(`/api/file/${pdfNodeId}`);

  // ページサイズ事前取得 (estimateSize 精度向上)
  const { pageSizes, isReady: pageSizesReady } = usePdfPageSizes(document);

  // 仮想スクロール（pageSizes 反映 + 初期スクロール + zoom anchor 維持）
  const anchorIndexRef = useRef(0);
  const { virtualizer, scrollRef, scrollElement } = useMangaVirtualizer({
    count: pageCount,
    zoomLevel,
    initialIndex: initialPage - 1,
    pageSizes,
    pageSizesReady,
    anchorIndexRef,
  });

  // スクロール位置からのページ検出
  const mangaScroll = useMangaScroll({
    virtualizer,
    scrollElement,
    totalCount: pageCount,
    scrollSpeed,
  });

  // zoom anchor 用に最新 index を ref に反映
  anchorIndexRef.current = mangaScroll.currentIndex;

  // currentIndex を URL に即時同期（PdfMangaViewer は initialPage 比較で発火回数が少ないため debounce 不要）
  // onIndexChange は 0-based、URL は 1-based に変換して onPageChange へ
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

  // ツールバー自動表示/非表示

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();

  // キーボードヘルプ
  const [isHelpOpen, setIsHelpOpen] = useState(false);

  // セット境界トースト（duration は useToast 内部 timer と Toast 側を同期）
  const { toastMessage, toastDuration, showToast, dismissToast } = useToast();

  // セット間ジャンプ + バックグラウンドプリフェッチ
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

  // キーボードショートカット
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
    // タイトル + 現在ページ / 総ページ を 3 秒トースト表示
    showTitle: () =>
      showToast(formatPageLabel(pdfName, mangaScroll.currentIndex + 1, pageCount), 3000),
  });

  // カーソルオートハイド（1秒 idle で消す）
  const { resetCursorTimer } = useCursorAutoHide(scrollRef);

  // 画像幅
  const imageWidth = `${zoomLevel}%`;

  // ローディング表示
  if (isLoading) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black">
        <p className="text-gray-400" data-testid="pdf-loading">
          PDF を読み込み中...
        </p>
      </div>
    );
  }

  // エラー表示
  if (error) {
    return (
      <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 bg-black">
        <p className="text-red-400" data-testid="pdf-error">
          PDF を開けません: {error.message}
        </p>
        <button
          type="button"
          onClick={onClose}
          className="rounded bg-surface-raised px-4 py-2 text-white hover:bg-surface-overlay"
        >
          閉じる
        </button>
      </div>
    );
  }

  if (!document) {
    return null;
  }

  return (
    <div data-testid="pdf-manga-viewer" className="fixed inset-0 z-50 flex bg-black">
      {/* メインエリア */}
      <div ref={containerCallbackRef} className="relative flex flex-1 flex-col overflow-hidden">
        {/* ツールバー（デスクトップ: 自動表示/非表示、タッチ: 常時表示・通常フロー） */}
        <div
          data-testid="toolbar-wrapper"
          className={
            isTouch
              ? "relative z-10"
              : `absolute top-0 right-0 left-0 z-10 transition-opacity duration-300 ${isToolbarVisible ? "opacity-100" : "pointer-events-none opacity-0"}`
          }
        >
          <MangaToolbar
            currentIndex={mangaScroll.currentIndex}
            totalCount={pageCount}
            zoomLevel={zoomLevel}
            scrollSpeed={scrollSpeed}
            setName={pdfName}
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
        </div>

        {/* 仮想スクロール PDF ページリスト */}
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
              width: imageWidth,
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
                {/* 描画キャッシュ (renderCache) は PdfCgViewer のみ適用
                    マンガモードは zoomLevel 変動でキャッシュヒット率が低く、
                    メモリ圧迫のリスクがあるため見送り (Phase 6.5 設計判断) */}
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

        {/* ページスライダー（右端フェードイン） */}
        <VerticalPageSlider
          currentIndex={mangaScroll.currentIndex}
          totalCount={pageCount}
          onGoTo={mangaScroll.scrollToImage}
          containerRef={scrollRef}
          onSliderActivity={resetCursorTimer}
        />

        {/* キーボードヘルプ */}
        {isHelpOpen && (
          <KeyboardHelp shortcuts={MANGA_SHORTCUTS} onClose={() => setIsHelpOpen(false)} />
        )}

        {/* セット間ジャンプの確認プロンプト */}
        {/* セット境界トースト */}
        {toastMessage && (
          <Toast message={toastMessage} onDismiss={dismissToast} duration={toastDuration} />
        )}

        {setJump.prompt && (
          <NavigationPrompt
            message={setJump.prompt.message}
            onConfirm={setJump.prompt.onConfirm}
            onCancel={setJump.prompt.onCancel}
            extraConfirmKeys={setJump.prompt.extraConfirmKeys}
          />
        )}
      </div>
    </div>
  );
}
