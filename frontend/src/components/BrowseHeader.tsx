// ファイルブラウザーのヘッダー
// - サイドバートグルボタン
// - 「← トップ」ナビゲーション
// - 現在パス表示
// - モード切替トグル（CG / マンガ）
// - 検索バー

import { useNavigate } from "react-router-dom";
import type { ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { ModeToggle } from "./ModeToggle";
import { SearchBar } from "./SearchBar";

interface BrowseHeaderProps {
  currentName: string;
  mode: ViewerMode;
  onModeChange: (mode: ViewerMode) => void;
}

export function BrowseHeader({ currentName, mode, onModeChange }: BrowseHeaderProps) {
  const navigate = useNavigate();
  const toggleSidebar = useViewerStore((s) => s.toggleSidebar);

  return (
    <header className="flex items-center gap-4 border-b border-white/5 bg-surface-card p-4">
      <button
        type="button"
        onClick={toggleSidebar}
        className="rounded-lg px-2 py-1.5 text-lg text-gray-300 transition-colors hover:bg-surface-raised hover:text-white"
        aria-label="サイドバー切替"
      >
        &#x2261;
      </button>
      <button
        type="button"
        onClick={() => navigate("/")}
        className="rounded-lg px-3 py-1.5 text-sm text-gray-300 transition-colors hover:bg-surface-raised hover:text-white"
      >
        ← トップ
      </button>
      <span className="min-w-0 truncate text-sm text-gray-400">{currentName}</span>
      <div className="ml-auto flex items-center gap-4">
        <ModeToggle mode={mode} onModeChange={onModeChange} />
        <div className="w-64">
          <SearchBar />
        </div>
      </div>
    </header>
  );
}
