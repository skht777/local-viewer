// ビューワーモード切替トグル（CG / マンガ）
// - BrowseHeader に配置し、ビューワーを開く前にモードを選択する
// - CgToolbar の aria-pressed ボタンスタイルを踏襲

import type { ViewerMode } from "../hooks/useViewerParams";

interface ModeToggleProps {
  mode: ViewerMode;
  onModeChange: (mode: ViewerMode) => void;
}

export function ModeToggle({ mode, onModeChange }: ModeToggleProps) {
  return (
    <div role="group" aria-label="ビューワーモード" className="flex rounded-lg bg-gray-900">
      <button
        type="button"
        data-testid="mode-toggle-cg"
        onClick={() => onModeChange("cg")}
        className={`rounded-l-lg px-3 py-1 text-xs font-medium transition-colors ${
          mode === "cg"
            ? "bg-blue-600 text-white"
            : "text-gray-400 hover:bg-gray-700 hover:text-gray-200"
        }`}
        aria-pressed={mode === "cg"}
      >
        CG
      </button>
      <button
        type="button"
        data-testid="mode-toggle-manga"
        onClick={() => onModeChange("manga")}
        className={`rounded-r-lg px-3 py-1 text-xs font-medium transition-colors ${
          mode === "manga"
            ? "bg-blue-600 text-white"
            : "text-gray-400 hover:bg-gray-700 hover:text-gray-200"
        }`}
        aria-pressed={mode === "manga"}
      >
        マンガ
      </button>
    </div>
  );
}
