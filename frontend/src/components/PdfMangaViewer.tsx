// PDF マンガモードビューワー: 全ページを仮想スクロールで表示
// - usePdfPageSizes で estimateSize の精度を保証
// - PdfCanvas で各ページを canvas 描画
// - MangaToolbar, PageCounter, キーボード等を再利用

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useMangaScroll } from "../hooks/useMangaScroll";
import { useMangaKeyboard } from "../hooks/useMangaKeyboard";
import { useSetJump } from "../hooks/useSetJump";
import { usePdfDocument } from "../hooks/usePdfDocument";
import { usePdfPageSizes } from "../hooks/usePdfPageSizes";
import { PdfCanvas } from "./PdfCanvas";
import { PdfPageSidebar } from "./PdfPageSidebar";
import { MangaToolbar } from "./MangaToolbar";
import { NavigationPrompt } from "./NavigationPrompt";
import { PageCounter } from "./PageCounter";

interface PdfMangaViewerProps {
  pdfNodeId: string;
  pdfName: string;
  parentNodeId: string | null;
  initialPage: number;
  mode: ViewerMode;
  onPageChange: (page: number) => void;
  onModeChange: (mode: ViewerMode) => void;
  onClose: () => void;
}

export function PdfMangaViewer({
  pdfNodeId,
  pdfName,
  parentNodeId,
  initialPage,
  mode,
  onPageChange,
  onModeChange,
  onClose,
}: PdfMangaViewerProps) {
  const zoomLevel = useViewerStore((s) => s.zoomLevel);
  const setZoomLevel = useViewerStore((s) => s.setZoomLevel);
  const zoomIn = useViewerStore((s) => s.zoomIn);
  const zoomOut = useViewerStore((s) => s.zoomOut);
  const scrollSpeed = useViewerStore((s) => s.scrollSpeed);
  const setScrollSpeed = useViewerStore((s) => s.setScrollSpeed);
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
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

  // currentIndex を URL に同期
  useEffect(() => {
    onPageChange(mangaScroll.currentIndex + 1); // 1-based
  }, [mangaScroll.currentIndex, onPageChange]);

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

  // セット間ジャンプ
  const setJump = useSetJump({
    currentNodeId: pdfNodeId,
    parentNodeId,
    mode,
  });

  // Escape 優先順位
  const handleEscape = useCallback(() => {
    if (setJump.prompt) {
      setJump.dismissPrompt();
      return;
    }
    if (isFullscreen) {
      globalThis.document.exitFullscreen();
      return;
    }
    onClose();
  }, [setJump, isFullscreen, onClose]);

  // キーボードショートカット
  useMangaKeyboard({
    scrollUp: mangaScroll.scrollUp,
    scrollDown: mangaScroll.scrollDown,
    scrollToTop: mangaScroll.scrollToTop,
    scrollToBottom: mangaScroll.scrollToBottom,
    onEscape: handleEscape,
    toggleFullscreen,
    toggleMode: () => onModeChange(mode === "manga" ? "cg" : "manga"),
    goNextSet: setJump.goNextSet,
    goPrevSet: setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
    zoomIn,
    zoomOut,
    zoomReset: () => setZoomLevel(100),
  });

  // カーソルオートハイド
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const handleMouseMove = useCallback(() => {
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
          className="rounded bg-gray-700 px-4 py-2 text-white hover:bg-gray-600"
        >
          閉じる
        </button>
      </div>
    );
  }

  if (!document) return null;

  return (
    <div className="fixed inset-0 z-50 flex bg-black">
      {/* ページ番号サイドバー (instant 追従) */}
      {isSidebarOpen && (
        <PdfPageSidebar
          pageCount={pageCount}
          currentIndex={mangaScroll.currentIndex}
          onSelect={mangaScroll.scrollToImage}
          scrollBehavior="instant"
        />
      )}

      {/* メインエリア */}
      <div className="relative flex flex-1 flex-col overflow-hidden">
        {/* ツールバー */}
        <MangaToolbar
          currentIndex={mangaScroll.currentIndex}
          totalCount={pageCount}
          zoomLevel={zoomLevel}
          scrollSpeed={scrollSpeed}
          onScrollToImage={mangaScroll.scrollToImage}
          onZoomIn={zoomIn}
          onZoomOut={zoomOut}
          onZoomChange={setZoomLevel}
          onScrollSpeedChange={setScrollSpeed}
          onToggleFullscreen={toggleFullscreen}
          onClose={onClose}
        />

        {/* 仮想スクロール PDF ページリスト */}
        <div
          ref={scrollRef}
          data-testid="pdf-manga-scroll-area"
          className="flex-1 overflow-auto"
          onMouseMove={handleMouseMove}
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
                <PdfCanvas
                  document={document}
                  pageNumber={virtualRow.index + 1}
                  fitMode="width"
                  containerWidth={scrollRef.current?.clientWidth ?? 800}
                  containerHeight={scrollRef.current?.clientHeight ?? 600}
                />
              </div>
            ))}
          </div>
        </div>

        {/* ページカウンター */}
        <PageCounter setName={pdfName} current={mangaScroll.currentIndex + 1} total={pageCount} />

        {/* セット間ジャンプの確認プロンプト */}
        {setJump.prompt && (
          <NavigationPrompt
            message={setJump.prompt.message}
            onConfirm={setJump.prompt.onConfirm}
            onCancel={setJump.prompt.onCancel}
          />
        )}
      </div>
    </div>
  );
}
