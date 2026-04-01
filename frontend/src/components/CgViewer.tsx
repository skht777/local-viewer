// CGモード本体: 画像1枚 or 見開き表示 + ツールバー
// - spreadMode に応じた 1 枚 / 2 枚横並び表示
// - fitMode に応じた画像サイズ制御（小さい画像も拡大表示）
// - 画像クリックでページ送り（画面中央分割: 右半分→次、左半分→前）
// - カーソルオートハイド（1秒 idle → cursor: none）
// - セット間ジャンプ（PageDown/X, Shift+X 等）
// - Escape 優先順位: プロンプト → フルスクリーン → ビューワー閉じ

import { useCallback, useRef, useState } from "react";
import type { AncestorEntry, BrowseEntry } from "../types/api";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useCgNavigation } from "../hooks/useCgNavigation";
import { useCgKeyboard } from "../hooks/useCgKeyboard";
import { useImagePreload } from "../hooks/useImagePreload";
import { useSetJump } from "../hooks/useSetJump";
import { useToast } from "../hooks/useToast";
import { useToolbarAutoHide } from "../hooks/useToolbarAutoHide";
import type { ViewerMode } from "../hooks/useViewerParams";
import { CgToolbar } from "./CgToolbar";
import { KeyboardHelp, CG_SHORTCUTS } from "./KeyboardHelp";
import { NavigationPrompt } from "./NavigationPrompt";
import { PageSlider } from "./PageSlider";
import { Toast } from "./Toast";

interface CgViewerProps {
  images: BrowseEntry[];
  currentIndex: number;
  setName: string;
  parentNodeId: string | null;
  currentNodeId: string | null;
  ancestors?: AncestorEntry[];
  mode: ViewerMode;
  onIndexChange: (index: number) => void;
  onClose: () => void;
}

// fitMode + 見開きページ数に応じた画像 CSS クラス
function fitClass(fitMode: string, pageCount: number): string {
  const isSpread = pageCount > 1;
  switch (fitMode) {
    case "width":
      return isSpread ? "h-auto max-w-full object-contain" : "w-full h-auto object-contain";
    case "height":
      return isSpread ? "h-full w-auto object-contain" : "h-full w-auto object-contain";
    case "original":
      return "max-w-none max-h-none";
    default:
      return isSpread ? "h-auto max-w-full object-contain" : "w-full h-auto object-contain";
  }
}

export function CgViewer({
  images,
  currentIndex,
  setName,
  parentNodeId,
  currentNodeId,
  ancestors,
  mode,
  onIndexChange,
  onClose,
}: CgViewerProps) {
  const fitMode = useViewerStore((s) => s.fitMode);
  const spreadMode = useViewerStore((s) => s.spreadMode);
  const setFitMode = useViewerStore((s) => s.setFitMode);
  const cycleSpreadMode = useViewerStore((s) => s.cycleSpreadMode);
  const { isFullscreen, toggleFullscreen } = useFullscreen();
  const nav = useCgNavigation(images.length, currentIndex, onIndexChange, spreadMode);

  // 隣接画像プリフェッチ (見開き時は range を拡大)
  const preloadRange = spreadMode === "single" ? 2 : 4;
  useImagePreload(images, currentIndex, preloadRange);

  // 画像境界トースト
  const { toastMessage, showToast, dismissToast } = useToast();

  // 境界チェック付きナビゲーション
  const handleGoNext = useCallback(() => {
    if (!nav.canGoNext) {
      showToast("最後の画像です");
      return;
    }
    nav.goNext();
  }, [nav, showToast]);

  const handleGoPrev = useCallback(() => {
    if (!nav.canGoPrev) {
      showToast("最初の画像です");
      return;
    }
    nav.goPrev();
  }, [nav, showToast]);

  // キーボードヘルプ
  const [isHelpOpen, setIsHelpOpen] = useState(false);

  // セット間ジャンプ
  const setJump = useSetJump({ currentNodeId, parentNodeId, ancestors, mode });

  // Escape 優先順位: (1) ヘルプ閉じ → (2) プロンプト閉じ → (3) フルスクリーン解除 → (4) ビューワー閉じ
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
      document.exitFullscreen();
      return;
    }
    onClose();
  }, [isHelpOpen, setJump, isFullscreen, onClose]);

  // キーボードショートカット
  useCgKeyboard({
    goNext: handleGoNext,
    goPrev: handleGoPrev,
    goFirst: nav.goFirst,
    goLast: nav.goLast,
    onEscape: handleEscape,
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
  });

  // ツールバー自動表示/非表示

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();

  // カーソルオートハイド
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const imageAreaRef = useRef<HTMLDivElement>(null);

  // カーソルオートハイドをリセット（スライダー操作時にも呼ばれる）
  const resetCursorTimer = useCallback(() => {
    if (imageAreaRef.current) {
      imageAreaRef.current.style.cursor = "";
    }
    clearTimeout(cursorTimerRef.current);
    cursorTimerRef.current = setTimeout(() => {
      if (imageAreaRef.current) {
        imageAreaRef.current.style.cursor = "none";
      }
    }, 1000);
  }, []);

  // 画像クリックでページ送り（画面中央分割: 右半分→次、左半分→前）
  const handleImageClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const mid = rect.left + rect.width / 2;
      if (e.clientX > mid) {
        handleGoNext();
      } else {
        handleGoPrev();
      }
    },
    [handleGoNext, handleGoPrev],
  );

  const { displayIndices } = nav;
  if (displayIndices.length === 0) return null;

  // ページカウンター: 見開き時は "3-4 / 12" 形式
  const firstDisplay = displayIndices[0] + 1;
  const lastDisplay = displayIndices[displayIndices.length - 1] + 1;
  const currentEnd = displayIndices.length > 1 ? lastDisplay : undefined;

  return (
    <div data-testid="cg-viewer" className="fixed inset-0 z-50 flex bg-black">
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
            currentIndex={currentIndex}
            totalCount={images.length}
            setName={setName}
            currentPage={firstDisplay}
            currentPageEnd={currentEnd}
            onFitWidth={() => setFitMode("width")}
            onFitHeight={() => setFitMode("height")}
            onCycleSpread={cycleSpreadMode}
            onToggleFullscreen={toggleFullscreen}
            onGoTo={nav.goTo}
            onClose={onClose}
          />
        </div>

        {/* 画像表示エリア */}
        <div
          ref={imageAreaRef}
          data-testid="cg-image-area"
          className="flex flex-1 items-center justify-center overflow-auto"
          onClick={handleImageClick}
          onMouseMove={resetCursorTimer}
        >
          {displayIndices.map((idx) => {
            const img = images[idx];
            if (!img) return null;
            return (
              <div
                key={img.node_id}
                className={
                  displayIndices.length > 1
                    ? "flex min-w-0 flex-1 items-center justify-center"
                    : "flex items-center justify-center"
                }
              >
                <img
                  src={`/api/file/${img.node_id}`}
                  alt={img.name}
                  className={fitClass(fitMode, displayIndices.length)}
                  draggable={false}
                />
              </div>
            );
          })}
        </div>

        {/* ページスライダー（下部フェードイン） */}
        <PageSlider
          currentIndex={currentIndex}
          totalCount={images.length}
          onGoTo={nav.goTo}
          containerRef={imageAreaRef}
          onSliderActivity={resetCursorTimer}
        />

        {/* 画像境界トースト */}
        {toastMessage && <Toast message={toastMessage} onDismiss={dismissToast} />}

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
