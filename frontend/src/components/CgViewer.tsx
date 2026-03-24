// CGモード本体: 画像1枚表示 + ツールバー + ページカウンター + サムネイルサイドバー
// - fitMode に応じた画像サイズ制御（小さい画像も拡大表示）
// - 画像クリックでページ送り（右半分→次、左半分→前）
// - カーソルオートハイド（1秒 idle → cursor: none）

import { useCallback, useRef } from "react";
import type { BrowseEntry } from "../types/api";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useCgNavigation } from "../hooks/useCgNavigation";
import { useImagePreload } from "../hooks/useImagePreload";
import { CgToolbar } from "./CgToolbar";
import { PageCounter } from "./PageCounter";
import { ThumbnailSidebar } from "./ThumbnailSidebar";

interface CgViewerProps {
  images: BrowseEntry[];
  currentIndex: number;
  setName: string;
  parentNodeId: string | null;
  onIndexChange: (index: number) => void;
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

export function CgViewer({ images, currentIndex, setName, onIndexChange, onClose }: CgViewerProps) {
  const fitMode = useViewerStore((s) => s.fitMode);
  const spreadMode = useViewerStore((s) => s.spreadMode);
  const setFitMode = useViewerStore((s) => s.setFitMode);
  const cycleSpreadMode = useViewerStore((s) => s.cycleSpreadMode);
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { toggleFullscreen } = useFullscreen();
  const nav = useCgNavigation(images.length, currentIndex, onIndexChange);

  // 隣接画像プリフェッチ
  useImagePreload(images, currentIndex);

  // カーソルオートハイド
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>();
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
      </div>
    </div>
  );
}
