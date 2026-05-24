// マンガモードのツールバー（3カラム構成）
// - 左: ページセレクト (lg) + ズームスライダー + スクロール速度 (lg)
// - 中央: ページカウンター
// - 右: フルスクリーン(F) + 閉じる + ⋯ メニュー (mobile のみ)
//
// mobile (lg 未満) 時の縮小:
// - ページセレクト / スクロール速度スライダーを hidden lg:flex で非表示
// - ⋯ メニュー (lg:hidden) で「最初」「最後」「ヘルプ」に到達可能化
// - safe-area: pl-safe-left / pr-safe-right / pt-safe-top で notch 衝突回避

import { formatPageLabel } from "../utils/formatPageLabel";
import { ToolbarOverflowMenu } from "./ToolbarOverflowMenu";

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
  // モバイル ⋯ メニュー用 (optional)
  onGoFirst?: () => void;
  onGoLast?: () => void;
  onToggleHelp?: () => void;
}

// セット間ジャンプボタンの共通スタイル (タッチターゲット 44px 確保のため py-2)
const setJumpBtnClass =
  "rounded px-3 py-2 text-sm text-gray-300 hover:bg-surface-raised " +
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
  onGoFirst,
  onGoLast,
  onToggleHelp,
}: MangaToolbarProps) {
  // mobile ⋯ メニュー項目
  // - フルスクリーン (mobile では収まらないため集約。iOS Safari は制限あるが Android は機能)
  // - Home / End / Help (キーボードがない端末向け)
  const overflowItems = [
    { label: "フルスクリーン", onClick: onToggleFullscreen, "data-testid": "overflow-fullscreen" },
    onGoFirst && { label: "最初へ (Home)", onClick: onGoFirst, "data-testid": "overflow-home" },
    onGoLast && { label: "最後へ (End)", onClick: onGoLast, "data-testid": "overflow-end" },
    onToggleHelp && {
      label: "ヘルプ (?)",
      onClick: onToggleHelp,
      "data-testid": "overflow-help",
    },
  ].filter(Boolean) as { label: string; onClick: () => void; "data-testid": string }[];

  return (
    <div className="flex items-center gap-1 bg-black/50 px-2 py-2 pt-safe-top pl-safe-left pr-safe-right backdrop-blur-md lg:gap-3 lg:px-4">
      {/* 左: コントロール群 (shrink-0 で潰れない) */}
      <div className="flex shrink-0 items-center gap-1 lg:gap-3">
        {/* ページセレクト: モバイルでは非表示 */}
        <select
          value={currentIndex}
          onChange={(e) => onScrollToImage(Number(e.target.value))}
          className="hidden rounded bg-surface-raised px-3 py-2 text-sm text-white lg:block"
          aria-label="ページ選択"
        >
          {Array.from({ length: totalCount }, (_, i) => (
            <option key={i} value={i}>
              Page {i + 1}
            </option>
          ))}
        </select>

        {/* ズーム +/- は mobile でも表示。スライダーは lg のみ (スペース節約) */}
        <button
          type="button"
          onClick={onZoomOut}
          className="rounded px-2 py-2 text-sm text-gray-300 hover:bg-surface-raised lg:px-3"
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
          className="hidden w-28 lg:inline-block"
          aria-label="ズーム"
        />
        <span
          className="min-w-[3rem] text-center text-xs font-mono tabular-nums text-gray-300 lg:min-w-[3.5rem] lg:text-sm"
          data-testid="manga-zoom-level"
        >
          {zoomLevel}%
        </span>
        <button
          type="button"
          onClick={onZoomIn}
          className="rounded px-2 py-2 text-sm text-gray-300 hover:bg-surface-raised lg:px-3"
          aria-label="ズームイン"
        >
          +
        </button>

        {/* スクロール速度スライダー: モバイルでは非表示 (優先度低 + スペース節約) */}
        <input
          type="range"
          min={0.5}
          max={3}
          step={0.5}
          value={scrollSpeed}
          onChange={(e) => onScrollSpeedChange(Number(e.target.value))}
          className="hidden w-20 lg:block"
          aria-label="スクロール速度"
        />
        <span
          className="hidden text-sm font-mono tabular-nums text-gray-300 lg:inline"
          data-testid="manga-scroll-speed-label"
        >
          {scrollSpeed}x
        </span>
      </div>

      {/* 中央: 前セット + ページカウンター + 次セット (min-w-0 で潰れ許容) */}
      <div className="flex min-w-0 flex-1 items-center justify-center gap-1 lg:gap-2">
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
          className="min-w-0 truncate text-center text-xs font-mono tabular-nums text-gray-300 lg:text-sm"
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

      {/* 右: フルスクリーン (lg のみ) + 閉じる + ⋯ (mobile) */}
      <div className="flex shrink-0 items-center gap-1 lg:gap-3">
        <button
          type="button"
          onClick={onToggleFullscreen}
          className="hidden rounded px-3 py-2 text-sm text-gray-300 hover:bg-surface-raised lg:inline-flex"
          aria-label="フルスクリーン"
        >
          F
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded px-2 py-2 text-sm text-gray-300 hover:bg-surface-raised lg:px-3"
          aria-label="閉じる"
        >
          ✕
        </button>
        {overflowItems.length > 0 && (
          <ToolbarOverflowMenu items={overflowItems} className="lg:hidden" />
        )}
      </div>
    </div>
  );
}
