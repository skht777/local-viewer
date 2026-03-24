// CGモードのツールバー
// - フィット切替(V: 幅, H: 高さ) + 見開き切替(Q) + フルスクリーン(F) + 閉じる
// - ページセレクト: <select> ドロップダウンでページ直接ジャンプ

import type { FitMode, SpreadMode } from "../stores/viewerStore";

interface CgToolbarProps {
  fitMode: FitMode;
  spreadMode: SpreadMode;
  currentIndex: number;
  totalCount: number;
  onFitWidth: () => void;
  onFitHeight: () => void;
  onCycleSpread: () => void;
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
  spreadMode,
  currentIndex,
  totalCount,
  onFitWidth,
  onFitHeight,
  onCycleSpread,
  onToggleFullscreen,
  onGoTo,
  onClose,
}: CgToolbarProps) {
  return (
    <div className="absolute top-0 right-0 left-0 z-10 flex items-center gap-2 bg-black/60 px-3 py-2">
      {/* フィット切替 */}
      <button
        type="button"
        onClick={onFitWidth}
        className={`rounded px-2 py-1 text-xs ${fitMode === "width" ? "bg-blue-600 text-white" : "text-gray-300 hover:bg-gray-700"}`}
        aria-label="幅フィット"
      >
        W
      </button>
      <button
        type="button"
        onClick={onFitHeight}
        className={`rounded px-2 py-1 text-xs ${fitMode === "height" ? "bg-blue-600 text-white" : "text-gray-300 hover:bg-gray-700"}`}
        aria-label="高さフィット"
      >
        H
      </button>

      {/* 見開き切替 */}
      <button
        type="button"
        onClick={onCycleSpread}
        className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
        aria-label="見開き切替"
      >
        {spreadLabel(spreadMode)}
      </button>

      {/* ページセレクト */}
      <select
        value={currentIndex}
        onChange={(e) => onGoTo(Number(e.target.value))}
        className="rounded bg-gray-800 px-2 py-1 text-xs text-white"
      >
        {Array.from({ length: totalCount }, (_, i) => (
          <option key={i} value={i}>
            Page {i + 1}
          </option>
        ))}
      </select>

      <div className="flex-1" />

      {/* フルスクリーン */}
      <button
        type="button"
        onClick={onToggleFullscreen}
        className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
        aria-label="フルスクリーン"
      >
        F
      </button>

      {/* 閉じる */}
      <button
        type="button"
        onClick={onClose}
        className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
        aria-label="閉じる"
      >
        ✕
      </button>
    </div>
  );
}
