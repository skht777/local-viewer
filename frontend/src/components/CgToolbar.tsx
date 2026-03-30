// CGモードのツールバー（3カラム構成）
// - 左: フィット切替(W, H) + 見開き切替(Q) + ページセレクト
// - 中央: ページカウンター（セット名 + ページ番号）
// - 右: フルスクリーン(F) + 閉じる

import type { FitMode, SpreadMode } from "../stores/viewerStore";
import { formatPageLabel } from "../utils/formatPageLabel";

interface CgToolbarProps {
  fitMode: FitMode;
  spreadMode?: SpreadMode;
  currentIndex: number;
  totalCount: number;
  showSpread?: boolean;
  setName: string;
  currentPage: number;
  currentPageEnd?: number;
  onFitWidth: () => void;
  onFitHeight: () => void;
  onCycleSpread?: () => void;
  onToggleFullscreen: () => void;
  onGoTo: (index: number) => void;
  onClose: () => void;
}

// 見開きモードの表示ラベル
function spreadLabel(mode: SpreadMode): string {
  switch (mode) {
    case "single":
      return "1";
    case "spread":
      return "2";
    case "spread-offset":
      return "2+";
  }
}

export function CgToolbar({
  fitMode,
  spreadMode = "single",
  currentIndex,
  totalCount,
  showSpread = true,
  setName,
  currentPage,
  currentPageEnd,
  onFitWidth,
  onFitHeight,
  onCycleSpread,
  onToggleFullscreen,
  onGoTo,
  onClose,
}: CgToolbarProps) {
  return (
    <div className="absolute top-0 right-0 left-0 z-10 flex items-center bg-black/50 backdrop-blur-md px-3 py-2">
      {/* 左: コントロール群 */}
      <div className="flex items-center gap-2">
        {/* フィット切替 */}
        <button
          type="button"
          onClick={onFitWidth}
          className={`rounded px-2 py-1 text-xs ${fitMode === "width" ? "bg-blue-600 text-white" : "text-gray-300 hover:bg-surface-raised"}`}
          aria-label="幅フィット"
          aria-pressed={fitMode === "width"}
        >
          ↔
        </button>
        <button
          type="button"
          onClick={onFitHeight}
          className={`rounded px-2 py-1 text-xs ${fitMode === "height" ? "bg-blue-600 text-white" : "text-gray-300 hover:bg-surface-raised"}`}
          aria-label="高さフィット"
          aria-pressed={fitMode === "height"}
        >
          ↕
        </button>

        {/* 見開き切替 */}
        {showSpread && (
          <button
            type="button"
            onClick={onCycleSpread}
            className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-surface-raised"
            aria-label="見開き切替"
            data-testid="cg-spread-btn"
          >
            {spreadLabel(spreadMode)}
          </button>
        )}

        {/* ページセレクト */}
        <select
          value={currentIndex}
          onChange={(e) => onGoTo(Number(e.target.value))}
          className="rounded bg-surface-raised px-2 py-1 text-xs text-white"
        >
          {Array.from({ length: totalCount }, (_, i) => (
            <option key={i} value={i}>
              Page {i + 1}
            </option>
          ))}
        </select>
      </div>

      {/* 中央: ページカウンター */}
      <span
        data-testid="page-counter"
        className="flex-1 truncate text-center text-xs font-mono tabular-nums text-gray-300"
      >
        {formatPageLabel(setName, currentPage, totalCount, currentPageEnd)}
      </span>

      {/* 右: フルスクリーン + 閉じる */}
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onToggleFullscreen}
          className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-surface-raised"
          aria-label="フルスクリーン"
        >
          F
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-surface-raised"
          aria-label="閉じる"
        >
          ✕
        </button>
      </div>
    </div>
  );
}
