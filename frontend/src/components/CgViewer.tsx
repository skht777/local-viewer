// CGモード本体: 画像1枚表示 + ツールバー + ページカウンター + サムネイルサイドバー
// - fitMode に応じた画像サイズ制御（小さい画像も拡大表示）
// - 画像クリックでページ送り（右半分→次、左半分→前）
// - カーソルオートハイド（1秒 idle → cursor: none）
// - セット間ジャンプ（PageDown/X, Shift+X 等）
// - Escape 優先順位: プロンプト → フルスクリーン → ビューワー閉じ

import { useCallback, useRef } from "react";
import type { BrowseEntry } from "../types/api";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useCgNavigation } from "../hooks/useCgNavigation";
import { useCgKeyboard } from "../hooks/useCgKeyboard";
import { useImagePreload } from "../hooks/useImagePreload";
import { useSetJump } from "../hooks/useSetJump";
import type { ViewerMode } from "../hooks/useViewerParams";
import { CgToolbar } from "./CgToolbar";
import { NavigationPrompt } from "./NavigationPrompt";
import { PageCounter } from "./PageCounter";
import { ThumbnailSidebar } from "./ThumbnailSidebar";

interface CgViewerProps {
  images: BrowseEntry[];
  currentIndex: number;
  setName: string;
  parentNodeId: string | null;
  currentNodeId: string | null;
  mode: ViewerMode;
  onIndexChange: (index: number) => void;
  onModeChange: (mode: ViewerMode) => void;
  onClose: () => void;
}

// fitMode に応じた画像 CSS クラス
function fitClass(fitMode: string): string {
  switch (fitMode) {
    case "width":
      return "w-full h-auto object-contain";
    case "height":
      return "h-full w-auto object-contain";
    case "original":
      return "max-w-none max-h-none";
    default:
      return "w-full h-auto object-contain";
  }
}

export function CgViewer({
  images,
  currentIndex,
  setName,
  parentNodeId,
  currentNodeId,
  mode,
  onIndexChange,
  onModeChange,
  onClose,
}: CgViewerProps) {
  const fitMode = useViewerStore((s) => s.fitMode);
  const spreadMode = useViewerStore((s) => s.spreadMode);
  const setFitMode = useViewerStore((s) => s.setFitMode);
  const cycleSpreadMode = useViewerStore((s) => s.cycleSpreadMode);
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { isFullscreen, toggleFullscreen } = useFullscreen();
  const nav = useCgNavigation(images.length, currentIndex, onIndexChange);

  // 隣接画像プリフェッチ
  useImagePreload(images, currentIndex);

  // セット間ジャンプ
  const setJump = useSetJump({ currentNodeId, parentNodeId, mode });

  // Escape 優先順位: (1) プロンプト閉じ → (2) フルスクリーン解除 → (3) ビューワー閉じ
  const handleEscape = useCallback(() => {
    if (setJump.prompt) {
      setJump.dismissPrompt();
      return;
    }
    if (isFullscreen) {
      document.exitFullscreen();
      return;
    }
    onClose();
  }, [setJump, isFullscreen, onClose]);

  // キーボードショートカット
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

  // 画像クリックでページ送り（右半分→次、左半分→前）
  const handleImageClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const mid = rect.left + rect.width / 2;
      if (e.clientX > mid) {
        nav.goNext();
      } else {
        nav.goPrev();
      }
    },
    [nav],
  );

  const currentImage = images[currentIndex];
  if (!currentImage) return null;

  return (
    <div className="fixed inset-0 z-50 flex bg-black">
      {/* サムネイルサイドバー */}
      {isSidebarOpen && (
        <ThumbnailSidebar images={images} currentIndex={currentIndex} onSelect={nav.goTo} />
      )}

      {/* メインエリア */}
      <div className="relative flex flex-1 flex-col overflow-hidden">
        {/* ツールバー */}
        <CgToolbar
          fitMode={fitMode}
          spreadMode={spreadMode}
          currentIndex={currentIndex}
          totalCount={images.length}
          onFitWidth={() => setFitMode("width")}
          onFitHeight={() => setFitMode("height")}
          onCycleSpread={cycleSpreadMode}
          onToggleFullscreen={toggleFullscreen}
          onGoTo={nav.goTo}
          onClose={onClose}
        />

        {/* 画像表示エリア */}
        <div
          ref={imageAreaRef}
          data-testid="cg-image-area"
          className={`flex flex-1 items-center justify-center overflow-auto ${fitMode === "original" ? "overflow-auto" : "overflow-hidden"}`}
          onClick={handleImageClick}
          onMouseMove={handleMouseMove}
        >
          <img
            src={`/api/file/${currentImage.node_id}`}
            alt={currentImage.name}
            className={fitClass(fitMode)}
            draggable={false}
          />
        </div>

        {/* ページカウンター */}
        <PageCounter setName={setName} current={currentIndex + 1} total={images.length} />

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
