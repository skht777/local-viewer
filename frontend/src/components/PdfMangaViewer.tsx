// PDF マンガモードビューワー: 全ページを仮想スクロールで表示
// - usePdfPageSizes で estimateSize の精度を保証
// - PdfCanvas で各ページを canvas 描画
// - MangaToolbar, PageCounter, キーボード等を再利用

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { AncestorEntry } from "../types/api";
import type { SortOrder, ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useMangaScroll } from "../hooks/useMangaScroll";
import { useMangaKeyboard } from "../hooks/useMangaKeyboard";
import { useSetJump } from "../hooks/useSetJump";
import { useSiblingPrefetch } from "../hooks/useSiblingPrefetch";
import { useToolbarAutoHide } from "../hooks/useToolbarAutoHide";
import { usePdfDocument } from "../hooks/usePdfDocument";
import { usePdfPageSizes } from "../hooks/usePdfPageSizes";
import { PdfCanvas } from "./PdfCanvas";
import { KeyboardHelp, MANGA_SHORTCUTS } from "./KeyboardHelp";
import { MangaToolbar } from "./MangaToolbar";
import { NavigationPrompt } from "./NavigationPrompt";
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
  const { isFullscreen, toggleFullscreen } = useFullscreen();

  // PDF ドキュメント読み込み
  const { document, pageCount, isLoading, error } = usePdfDocument(`/api/file/${pdfNodeId}`);

  // ページサイズ事前取得 (estimateSize 精度向上)
  const { pageSizes, isReady: pageSizesReady } = usePdfPageSizes(document);

  // スクロールコンテナ
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);
  useEffect(() => {
    setScrollElement(scrollRef.current);
  }, []);

  // 仮想スクロール
  const virtualizer = useVirtualizer({
    count: pageCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => {
      // ページサイズが取得済みなら正確な高さを返す
      if (pageSizesReady && pageSizes[index]) {
        const ps = pageSizes[index];
        const w = ((scrollRef.current?.clientWidth ?? 800) * zoomLevel) / 100;
        return w * (ps.height / ps.width);
      }
      // フォールバック: 3:4 比率
      const w = scrollRef.current?.clientWidth ?? 800;
      return (w * zoomLevel * 4) / (100 * 3);
    },
    overscan: 3,
  });

  // pageSizes 取得完了時に measure を再実行
  useEffect(() => {
    if (pageSizesReady) virtualizer.measure();
  }, [pageSizesReady, virtualizer]);

  // スクロール位置からのページ検出
  const mangaScroll = useMangaScroll({
    virtualizer,
    scrollElement,
    totalCount: pageCount,
    scrollSpeed,
  });

  // currentIndex を URL に同期（値が変化した場合のみ）
  useEffect(() => {
    const page = mangaScroll.currentIndex + 1; // 1-based
    if (page !== initialPage) {
      onPageChange(page);
    }
  }, [mangaScroll.currentIndex, initialPage, onPageChange]);

  // 初期表示で initialPage の位置にスクロール
  const initialScrollDone = useRef(false);
  useEffect(() => {
    if (!initialScrollDone.current && initialPage > 1 && pageCount > 0) {
      virtualizer.scrollToIndex(initialPage - 1, { align: "start" });
      initialScrollDone.current = true;
    }
  }, [initialPage, pageCount, virtualizer]);

  // ズーム変更時: スクロールアンカー維持
  const prevZoomLevel = useRef(zoomLevel);
  useEffect(() => {
    if (prevZoomLevel.current !== zoomLevel) {
      const anchorIndex = mangaScroll.currentIndex;
      virtualizer.measure();
      requestAnimationFrame(() => {
        virtualizer.scrollToIndex(anchorIndex, { align: "start" });
      });
      prevZoomLevel.current = zoomLevel;
    }
  }, [zoomLevel, virtualizer, mangaScroll.currentIndex]);

  // ツールバー自動表示/非表示

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();

  // キーボードヘルプ
  const [isHelpOpen, setIsHelpOpen] = useState(false);

  // セット間ジャンプ + バックグラウンドプリフェッチ
  const setJump = useSetJump({
    currentNodeId: pdfNodeId,
    parentNodeId,
    ancestors,
    mode,
    sort,
  });
  useSiblingPrefetch({ currentNodeId: pdfNodeId, parentNodeId, ancestors, sort });

  // Escape 優先順位: (1) ヘルプ閉じ → (2) プロンプト → (3) フルスクリーン → (4) ビューワー閉じ
  const handleEscape = useCallback(() => {
    if (isHelpOpen) {
      setIsHelpOpen(false);
      return;
    }
    if (setJump.prompt) {
      setJump.dismissPrompt();
      return;
    }
    if (isFullscreen) {
      globalThis.document.exitFullscreen();
      return;
    }
    onClose();
  }, [isHelpOpen, setJump, isFullscreen, onClose]);

  // キーボードショートカット
  useMangaKeyboard({
    scrollUp: mangaScroll.scrollUp,
    scrollDown: mangaScroll.scrollDown,
    scrollToTop: mangaScroll.scrollToTop,
    scrollToBottom: mangaScroll.scrollToBottom,
    onEscape: handleEscape,
    toggleFullscreen,
    goNextSet: setJump.prompt ? undefined : setJump.goNextSet,
    goPrevSet: setJump.prompt ? undefined : setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
    zoomIn,
    zoomOut,
    zoomReset: () => setZoomLevel(100),
    toggleHelp: () => setIsHelpOpen((prev) => !prev),
  });

  // カーソルオートハイド
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  // カーソルオートハイドをリセット（スライダー操作時にも呼ばれる）
  const resetCursorTimer = useCallback(() => {
    if (scrollRef.current) scrollRef.current.style.cursor = "";
    clearTimeout(cursorTimerRef.current);
    cursorTimerRef.current = setTimeout(() => {
      if (scrollRef.current) scrollRef.current.style.cursor = "none";
    }, 1000);
  }, []);

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

  if (!document) return null;

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
