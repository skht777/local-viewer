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
import { usePdfRenderCache } from "../hooks/usePdfRenderCache";
import { useViewerBoundaryNavigation } from "../hooks/useViewerBoundaryNavigation";
import { formatPageLabel } from "../utils/formatPageLabel";
import { PdfCanvas } from "./PdfCanvas";
import { CgToolbar } from "./CgToolbar";
import { KeyboardHelp, CG_SHORTCUTS } from "./KeyboardHelp";
import { NavigationPrompt } from "./NavigationPrompt";
import { PageSlider } from "./PageSlider";
import { Toast } from "./Toast";

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

// 受入: 残 12 行 / 4 statements 超過。useState + handler / displayIndices 派生値 /
// useCgKeyboard 設定オブジェクトの細粒度抽出は可読性低下と引き換えになるため、
// 「申し送り」へ載せる方針 (B-7 plan の B-6 同等フォールバック)。
// oxlint-disable-next-line max-lines-per-function, max-statements
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

  // PDF ドキュメント読み込み
  const { document, pageCount, isLoading, error } = usePdfDocument(`/api/file/${pdfNodeId}`);

  // 描画キャッシュ (PdfCgViewer のみ適用)
  const renderCache = usePdfRenderCache();

  // 現在ページ (0-based index で管理、表示は 1-based)
  const [currentPage, setCurrentPage] = useState(initialPage - 1);

  const handlePageChange = useCallback(
    (index: number) => {
      setCurrentPage(index);
      // URL は 1-based
      onPageChange(index + 1);
    },
    [onPageChange],
  );

  // ページナビゲーション (spread 対応)
  const nav = useCgNavigation(pageCount, currentPage, handlePageChange, spreadMode);

  // ツールバー自動表示/非表示

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();

  // ページ境界トースト（duration は useToast 内部 timer と Toast 側を同期）
  const { toastMessage, toastDuration, showToast, dismissToast } = useToast();

  // 境界チェック付きナビゲーション（PDF はメッセージを「ページ」表現に差し替え）
  const { handleGoNext, handleGoPrev } = useViewerBoundaryNavigation({
    nav,
    showToast,
    firstMessage: "最初のページです",
    lastMessage: "最後のページです",
  });

  // キーボードヘルプ
  const [isHelpOpen, setIsHelpOpen] = useState(false);

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

  // キーボードショートカット (spread 有効)
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

  // コンテナサイズ (fitMode 計算用) — ResizeObserver で動的計測
  const { containerSize, imageAreaRef, combinedRef } = usePdfContainerSize();

  // カーソルオートハイド（1秒 idle で消す）
  const { resetCursorTimer } = useCursorAutoHide(imageAreaRef);

  // 画像クリックでページ送り (画面中央分割: 右半分→次、左半分→前)
  const handleClick = useClickToTurnPage(handleGoNext, handleGoPrev);

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
    <div data-testid="pdf-cg-viewer" className="fixed inset-0 z-50 flex bg-black">
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
          <CgToolbar
            fitMode={fitMode}
            spreadMode={spreadMode}
            currentIndex={currentPage}
            totalCount={pageCount}
            showSpread={true}
            setName={pdfName}
            currentPage={firstDisplay}
            currentPageEnd={currentEnd}
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
        </div>

        {/* PDF ページ表示エリア */}
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
                containerWidth={pageContainerWidth}
                renderCache={renderCache}
                enableTextLayer={true}
                containerHeight={containerSize.height}
              />
            </div>
          ))}
        </div>

        {/* ページスライダー（下部フェードイン） */}
        <PageSlider
          currentIndex={currentPage}
          totalCount={pageCount}
          onGoTo={nav.goTo}
          containerRef={imageAreaRef}
          onSliderActivity={resetCursorTimer}
        />

        {/* ページ境界トースト */}
        {toastMessage && (
          <Toast message={toastMessage} onDismiss={dismissToast} duration={toastDuration} />
        )}

        {/* キーボードヘルプ */}
        {isHelpOpen && (
          <KeyboardHelp shortcuts={CG_SHORTCUTS} onClose={() => setIsHelpOpen(false)} />
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
