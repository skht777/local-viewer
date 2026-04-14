// ファイルブラウザーのヘッダー
// - サイドバートグルボタン
// - 「← トップ」ナビゲーション
// - パンくずリスト
// - モード切替トグル（CG / マンガ）
// - 検索バー

import { useNavigate } from "react-router-dom";
import type { ViewerMode } from "../hooks/useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import type { AncestorEntry } from "../types/api";
import { Breadcrumb } from "./Breadcrumb";
import { ModeToggle } from "./ModeToggle";
import { SearchBar } from "./SearchBar";

interface BrowseHeaderProps {
  currentName: string;
  ancestors: AncestorEntry[];
  onBreadcrumbSelect: (nodeId: string) => void;
  mode: ViewerMode;
  onModeChange: (mode: ViewerMode) => void;
  nodeId?: string;
}

export function BrowseHeader({
  currentName,
  ancestors,
  onBreadcrumbSelect,
  mode,
  onModeChange,
  nodeId,
}: BrowseHeaderProps) {
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
        className="shrink-0 rounded-lg px-3 py-1.5 text-sm text-gray-300 transition-colors hover:bg-surface-raised hover:text-white"
      >
        ← トップ
      </button>
      <Breadcrumb ancestors={ancestors} currentName={currentName} onSelect={onBreadcrumbSelect} />
      <div className="ml-auto flex shrink-0 items-center gap-4">
        <ModeToggle mode={mode} onModeChange={onModeChange} />
        <div className="w-80">
          <SearchBar scope={nodeId} />
        </div>
      </div>
    </header>
  );
}
