// PDF CG モード用ツールバー
// - CgToolbar から見開き (spread) ボタンを除外
// - フィット切替(V/H) + フルスクリーン(F) + ページセレクト + 閉じる

import type { FitMode } from "../stores/viewerStore";

interface PdfCgToolbarProps {
  fitMode: FitMode;
  currentIndex: number;
  totalCount: number;
  onFitWidth: () => void;
  onFitHeight: () => void;
  onToggleFullscreen: () => void;
  onGoTo: (index: number) => void;
  onClose: () => void;
}

export function PdfCgToolbar({
  fitMode,
  currentIndex,
  totalCount,
  onFitWidth,
  onFitHeight,
  onToggleFullscreen,
  onGoTo,
  onClose,
}: PdfCgToolbarProps) {
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
