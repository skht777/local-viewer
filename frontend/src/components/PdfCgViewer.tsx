// PDF CG モードビューワー: 1ページずつ表示
// - usePdfDocument で PDF 読み込み
// - PdfCanvas で canvas 描画
// - CgToolbar (showSpread=false), PageCounter, キーボード等を再利用
// - useSetJump: currentNodeId = pdfNodeId (PDF 自身)

import { useCallback, useRef, useState } from "react";
import type { ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useCgNavigation } from "../hooks/useCgNavigation";
import { useCgKeyboard } from "../hooks/useCgKeyboard";
import { useSetJump } from "../hooks/useSetJump";
import { usePdfDocument } from "../hooks/usePdfDocument";
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
  const setFitMode = useViewerStore((s) => s.setFitMode);
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { isFullscreen, toggleFullscreen } = useFullscreen();

  // PDF ドキュメント読み込み
  const { document, pageCount, isLoading, error } = usePdfDocument(`/api/file/${pdfNodeId}`);

  // 現在ページ (0-based index で管理、表示は 1-based)
  const [currentPage, setCurrentPage] = useState(initialPage - 1);

  const handlePageChange = useCallback(
    (index: number) => {
      setCurrentPage(index);
      onPageChange(index + 1); // URL は 1-based
    },
    [onPageChange],
  );

  // ページナビゲーション
  const nav = useCgNavigation(pageCount, currentPage, handlePageChange);

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

  // キーボードショートカット (spread は no-op)
  useCgKeyboard({
    goNext: nav.goNext,
    goPrev: nav.goPrev,
    goFirst: nav.goFirst,
    goLast: nav.goLast,
    onEscape: handleEscape,
    toggleFullscreen,
    setFitWidth: () => setFitMode("width"),
    setFitHeight: () => setFitMode("height"),
    cycleSpread: () => {},
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

  // 画像クリックでページ送り (右半分→次、左半分→前)
  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const mid = rect.left + rect.width / 2;
      if (e.clientX > mid) nav.goNext();
      else nav.goPrev();
    },
    [nav],
  );

  // コンテナサイズ (fitMode 計算用)
  const [containerSize, setContainerSize] = useState({ width: 800, height: 600 });
  const combinedRef = useCallback((node: HTMLDivElement | null) => {
    (imageAreaRef as React.MutableRefObject<HTMLDivElement | null>).current = node;
    if (node) {
      const w = node.clientWidth || 800;
      const h = node.clientHeight || 600;
      setContainerSize((prev) => {
        if (prev.width === w && prev.height === h) return prev;
        return { width: w, height: h };
      });
    }
  }, []);

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
      {/* ページ番号サイドバー */}
      {isSidebarOpen && (
        <PdfPageSidebar pageCount={pageCount} currentIndex={currentPage} onSelect={nav.goTo} />
      )}

      {/* メインエリア */}
      <div className="relative flex flex-1 flex-col overflow-hidden">
        {/* ツールバー (showSpread=false: 見開きボタン非表示) */}
        <CgToolbar
          fitMode={fitMode}
          currentIndex={currentPage}
          totalCount={pageCount}
          showSpread={false}
          onFitWidth={() => setFitMode("width")}
          onFitHeight={() => setFitMode("height")}
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
          <PdfCanvas
            document={document}
            pageNumber={currentPage + 1}
            fitMode={fitMode}
            containerWidth={containerSize.width}
            containerHeight={containerSize.height}
          />
        </div>

        {/* ページカウンター */}
        <PageCounter setName={pdfName} current={currentPage + 1} total={pageCount} />

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
