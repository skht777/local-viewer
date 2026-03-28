// マンガモードのツールバー（3カラム構成）
// - 左: ページセレクト + ズームスライダー + スクロール速度
// - 中央: ページカウンター
// - 右: フルスクリーン(F) + 閉じる

import { formatPageLabel } from "../utils/formatPageLabel";

interface MangaToolbarProps {
  currentIndex: number;
  totalCount: number;
  zoomLevel: number;
  scrollSpeed: number;
  setName: string;
  onScrollToImage: (index: number) => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomChange: (level: number) => void;
  onScrollSpeedChange: (speed: number) => void;
  onToggleFullscreen: () => void;
  onClose: () => void;
}

export function MangaToolbar({
  currentIndex,
  totalCount,
  zoomLevel,
  scrollSpeed,
  setName,
  onScrollToImage,
  onZoomIn,
  onZoomOut,
  onZoomChange,
  onScrollSpeedChange,
  onToggleFullscreen,
  onClose,
}: MangaToolbarProps) {
  return (
    <div className="absolute top-0 right-0 left-0 z-10 flex items-center bg-black/60 px-3 py-2">
      {/* 左: コントロール群 */}
      <div className="flex items-center gap-2">
        {/* ページセレクト */}
        <select
          value={currentIndex}
          onChange={(e) => onScrollToImage(Number(e.target.value))}
          className="rounded bg-gray-800 px-2 py-1 text-xs text-white"
          aria-label="ページ選択"
        >
          {Array.from({ length: totalCount }, (_, i) => (
            <option key={i} value={i}>
              Page {i + 1}
            </option>
          ))}
        </select>

        {/* ズームスライダー */}
        <button
          type="button"
          onClick={onZoomOut}
          className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
          aria-label="ズームアウト"
        >
          -
        </button>
        <input
          type="range"
          min={25}
          max={300}
          step={25}
          value={zoomLevel}
          onChange={(e) => onZoomChange(Number(e.target.value))}
          className="w-24"
          aria-label="ズーム"
        />
        <span
          className="min-w-[3rem] text-center text-xs font-mono tabular-nums text-gray-300"
          data-testid="manga-zoom-level"
        >
          {zoomLevel}%
        </span>
        <button
          type="button"
          onClick={onZoomIn}
          className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
          aria-label="ズームイン"
        >
          +
        </button>

        {/* スクロール速度スライダー */}
        <input
          type="range"
          min={0.5}
          max={3.0}
          step={0.5}
          value={scrollSpeed}
          onChange={(e) => onScrollSpeedChange(Number(e.target.value))}
          className="w-16"
          aria-label="スクロール速度"
        />
        <span
          className="text-xs font-mono tabular-nums text-gray-300"
          data-testid="manga-scroll-speed-label"
        >
          {scrollSpeed}x
        </span>
      </div>

      {/* 中央: ページカウンター */}
      <span
        data-testid="page-counter"
        className="flex-1 truncate text-center text-xs font-mono tabular-nums text-gray-300"
      >
        {formatPageLabel(setName, currentIndex + 1, totalCount)}
      </span>

      {/* 右: フルスクリーン + 閉じる */}
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onToggleFullscreen}
          className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
          aria-label="フルスクリーン"
        >
          F
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
          aria-label="閉じる"
        >
          ✕
        </button>
      </div>
    </div>
  );
}
