// PDF CG モードビューワー: 1ページ or 見開き表示
// - usePdfDocument で PDF 読み込み
// - PdfCanvas で canvas 描画
// - CgToolbar (showSpread=true), PageCounter, キーボード等を再利用
// - spreadMode に応じた 1 ページ / 2 ページ横並び表示
// - ResizeObserver でコンテナサイズを動的計測
// - useSetJump: currentNodeId = pdfNodeId (PDF 自身)

import { useCallback, useEffect, useRef, useState } from "react";
import type { ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useCgNavigation } from "../hooks/useCgNavigation";
import { useCgKeyboard } from "../hooks/useCgKeyboard";
import { useSetJump } from "../hooks/useSetJump";
import { usePdfDocument } from "../hooks/usePdfDocument";
import { usePdfRenderCache } from "../hooks/usePdfRenderCache";
import { PdfCanvas } from "./PdfCanvas";
import { CgToolbar } from "./CgToolbar";
import { PdfPageSidebar } from "./PdfPageSidebar";
import { NavigationPrompt } from "./NavigationPrompt";
import { PageCounter } from "./PageCounter";

interface PdfCgViewerProps {
  pdfNodeId: string;
  pdfName: string;
  parentNodeId: string | null;
  initialPage: number;
  mode: ViewerMode;
  onPageChange: (page: number) => void;
  onModeChange: (mode: ViewerMode) => void;
  onClose: () => void;
}

export function PdfCgViewer({
  pdfNodeId,
  pdfName,
  parentNodeId,
  initialPage,
  mode,
  onPageChange,
  onModeChange,
  onClose,
}: PdfCgViewerProps) {
  const fitMode = useViewerStore((s) => s.fitMode);
  const spreadMode = useViewerStore((s) => s.spreadMode);
  const setFitMode = useViewerStore((s) => s.setFitMode);
  const cycleSpreadMode = useViewerStore((s) => s.cycleSpreadMode);
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { isFullscreen, toggleFullscreen } = useFullscreen();

  // PDF ドキュメント読み込み
  const { document, pageCount, isLoading, error } = usePdfDocument(`/api/file/${pdfNodeId}`);

  // 描画キャッシュ (PdfCgViewer のみ適用)
  const renderCache = usePdfRenderCache();

  // 現在ページ (0-based index で管理、表示は 1-based)
  const [currentPage, setCurrentPage] = useState(initialPage - 1);

  const handlePageChange = useCallback(
    (index: number) => {
      setCurrentPage(index);
      onPageChange(index + 1); // URL は 1-based
    },
    [onPageChange],
  );

  // ページナビゲーション (spread 対応)
  const nav = useCgNavigation(pageCount, currentPage, handlePageChange, spreadMode);

  // セット間ジャンプ: currentNodeId = PDF 自身、parentNodeId = 親ディレクトリ
  const setJump = useSetJump({
    currentNodeId: pdfNodeId,
    parentNodeId,
    mode,
  });

  // Escape 優先順位: (1) プロンプト → (2) フルスクリーン → (3) ビューワー閉じ
  const handleEscape = useCallback(() => {
    if (setJump.prompt) {
      setJump.dismissPrompt();
      return;
    }
    if (isFullscreen) {
      window.document.exitFullscreen?.();
      return;
    }
    onClose();
  }, [setJump, isFullscreen, onClose]);

  // キーボードショートカット (spread 有効)
  useCgKeyboard({
    goNext: nav.goNext,
    goPrev: nav.goPrev,
    goFirst: nav.goFirst,
    goLast: nav.goLast,
    onEscape: handleEscape,
    toggleFullscreen,
    setFitWidth: () => setFitMode("width"),
    setFitHeight: () => setFitMode("height"),
    cycleSpread: cycleSpreadMode,
    scrollUp: () => {},
    scrollDown: () => {},
    toggleMode: () => onModeChange(mode === "cg" ? "manga" : "cg"),
    goNextSet: setJump.goNextSet,
    goPrevSet: setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
  });

  // カーソルオートハイド
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const imageAreaRef = useRef<HTMLDivElement>(null);
  const handleMouseMove = useCallback(() => {
    if (imageAreaRef.current) imageAreaRef.current.style.cursor = "";
    clearTimeout(cursorTimerRef.current);
    cursorTimerRef.current = setTimeout(() => {
      if (imageAreaRef.current) imageAreaRef.current.style.cursor = "none";
    }, 1000);
  }, []);

  // 画像クリックでページ送り (画面中央分割: 右半分→次、左半分→前)
  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const mid = rect.left + rect.width / 2;
      if (e.clientX > mid) nav.goNext();
      else nav.goPrev();
    },
    [nav],
  );

  // コンテナサイズ (fitMode 計算用) — ResizeObserver で動的計測
  const [containerSize, setContainerSize] = useState({ width: 800, height: 600 });
  const resizeObserverRef = useRef<ResizeObserver | null>(null);

  const combinedRef = useCallback((node: HTMLDivElement | null) => {
    // 既存の Observer をクリーンアップ
    resizeObserverRef.current?.disconnect();

    (imageAreaRef as React.MutableRefObject<HTMLDivElement | null>).current = node;
    if (!node) return;

    // 初期サイズ
    const w = node.clientWidth || 800;
    const h = node.clientHeight || 600;
    setContainerSize({ width: w, height: h });

    // ResizeObserver で動的追従
    resizeObserverRef.current = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setContainerSize((prev) => {
        if (prev.width === width && prev.height === height) return prev;
        return { width, height };
      });
    });
    resizeObserverRef.current.observe(node);
  }, []);

  // ResizeObserver クリーンアップ
  useEffect(() => {
    return () => resizeObserverRef.current?.disconnect();
  }, []);

  // 見開き時の各ページに渡す containerWidth
  const { displayIndices } = nav;
  const pageContainerWidth =
    displayIndices.length > 1 ? containerSize.width / 2 : containerSize.width;

  // ページカウンター: 見開き時は "3-4 / 12" 形式
  const firstDisplay = displayIndices.length > 0 ? displayIndices[0] + 1 : 1;
  const lastDisplay = displayIndices.length > 0 ? displayIndices[displayIndices.length - 1] + 1 : 1;
  const currentEnd = displayIndices.length > 1 ? lastDisplay : undefined;

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
    <div data-testid="pdf-cg-viewer" className="fixed inset-0 z-50 flex bg-black">
      {/* サムネイルサイドバー */}
      {isSidebarOpen && (
        <PdfPageSidebar
          document={document}
          pageCount={pageCount}
          currentIndex={currentPage}
          onSelect={nav.goTo}
        />
      )}

      {/* メインエリア */}
      <div className="relative flex flex-1 flex-col overflow-hidden">
        {/* ツールバー (showSpread=true: 見開きボタン表示) */}
        <CgToolbar
          fitMode={fitMode}
          spreadMode={spreadMode}
          currentIndex={currentPage}
          totalCount={pageCount}
          showSpread={true}
          onFitWidth={() => setFitMode("width")}
          onFitHeight={() => setFitMode("height")}
          onCycleSpread={cycleSpreadMode}
          onToggleFullscreen={toggleFullscreen}
          onGoTo={nav.goTo}
          onClose={onClose}
        />

        {/* PDF ページ表示エリア */}
        <div
          ref={combinedRef}
          data-testid="pdf-cg-page-area"
          className={`flex flex-1 items-center justify-center ${fitMode === "original" ? "overflow-auto" : "overflow-hidden"}`}
          onClick={handleClick}
          onMouseMove={handleMouseMove}
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
                containerWidth={pageContainerWidth}
                renderCache={renderCache}
                enableTextLayer={true}
                containerHeight={containerSize.height}
              />
            </div>
          ))}
        </div>

        {/* ページカウンター */}
        <PageCounter
          setName={pdfName}
          current={firstDisplay}
          currentEnd={currentEnd}
          total={pageCount}
        />

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
