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
  onPrevSet: () => void;
  onNextSet: () => void;
  isSetJumpDisabled: boolean;
}

// セット間ジャンプボタンの共通スタイル（CgToolbar と同形）
const setJumpBtnClass =
  "rounded px-3 py-1.5 text-sm text-gray-300 hover:bg-surface-raised " +
  "disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent";

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
  onPrevSet,
  onNextSet,
  isSetJumpDisabled,
}: MangaToolbarProps) {
  return (
    <div className="flex items-center bg-black/50 backdrop-blur-md px-4 py-2.5">
      {/* 左: コントロール群 */}
      <div className="flex items-center gap-3">
        {/* ページセレクト */}
        <select
          value={currentIndex}
          onChange={(e) => onScrollToImage(Number(e.target.value))}
          className="rounded bg-surface-raised px-3 py-1.5 text-sm text-white"
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
          className="rounded px-3 py-1.5 text-sm text-gray-300 hover:bg-surface-raised"
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
          className="w-28"
          aria-label="ズーム"
        />
        <span
          className="min-w-[3.5rem] text-center text-sm font-mono tabular-nums text-gray-300"
          data-testid="manga-zoom-level"
        >
          {zoomLevel}%
        </span>
        <button
          type="button"
          onClick={onZoomIn}
          className="rounded px-3 py-1.5 text-sm text-gray-300 hover:bg-surface-raised"
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
          className="w-20"
          aria-label="スクロール速度"
        />
        <span
          className="text-sm font-mono tabular-nums text-gray-300"
          data-testid="manga-scroll-speed-label"
        >
          {scrollSpeed}x
        </span>
      </div>

      {/* 中央: 前セット + ページカウンター + 次セット */}
      <div className="flex flex-1 items-center justify-center gap-2">
        <button
          type="button"
          onClick={onPrevSet}
          disabled={isSetJumpDisabled}
          className={setJumpBtnClass}
          aria-label="前のセットへ"
          title="前のセット (Z / PageUp)"
          data-testid="manga-prev-set-btn"
        >
          ⏪
        </button>
        <span
          data-testid="page-counter"
          className="max-w-[60%] truncate text-center text-sm font-mono tabular-nums text-gray-300"
        >
          {formatPageLabel(setName, currentIndex + 1, totalCount)}
        </span>
        <button
          type="button"
          onClick={onNextSet}
          disabled={isSetJumpDisabled}
          className={setJumpBtnClass}
          aria-label="次のセットへ"
          title="次のセット (X / PageDown)"
          data-testid="manga-next-set-btn"
        >
          ⏩
        </button>
      </div>

      {/* 右: フルスクリーン + 閉じる */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={onToggleFullscreen}
          className="rounded px-3 py-1.5 text-sm text-gray-300 hover:bg-surface-raised"
          aria-label="フルスクリーン"
        >
          F
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded px-3 py-1.5 text-sm text-gray-300 hover:bg-surface-raised"
          aria-label="閉じる"
        >
          ✕
        </button>
      </div>
    </div>
  );
}
