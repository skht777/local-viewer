// ファイルブラウザーのヘッダー
// - サイドバートグルボタン (mobile: ハンバーガー)
// - 「← トップ」ナビゲーション
// - パンくずリスト
// - モード切替トグル（CG / マンガ）
// - 検索バー
// - レイアウト:
//   - lg 以上: 1 段 (ハンバーガー + 戻る + パンくず + 右側に Mode + Search)
//   - lg 未満: 2 段 (1 段目はナビ系、2 段目に Mode + Search)

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
    <header className="border-b border-white/5 bg-surface-card">
      {/* 1 段目: ナビ系 (常時表示) + lg 以上では右側に Mode/Search も並ぶ */}
      <div className="flex items-center gap-2 px-3 py-2 lg:gap-4 lg:p-4">
        <button
          type="button"
          onClick={toggleSidebar}
          data-testid="sidebar-toggle"
          className="rounded-lg px-3 py-2.5 text-lg text-gray-300 transition-colors hover:bg-surface-raised hover:text-white"
          aria-label="サイドバー切替"
        >
          &#x2261;
        </button>
        <button
          type="button"
          onClick={() => navigate("/")}
          className="shrink-0 rounded-lg px-3 py-2.5 text-sm text-gray-300 transition-colors hover:bg-surface-raised hover:text-white"
        >
          ← トップ
        </button>
        <Breadcrumb ancestors={ancestors} currentName={currentName} onSelect={onBreadcrumbSelect} />
        {/* lg 以上のみ表示: Mode + Search を右側に並べる */}
        <div className="ml-auto hidden shrink-0 items-center gap-4 lg:flex">
          <ModeToggle mode={mode} onModeChange={onModeChange} />
          <div className="w-80">
            <SearchBar scope={nodeId} />
          </div>
        </div>
      </div>
      {/* 2 段目: モバイル時のみ表示 (Mode + Search を横並びで縦積み) */}
      <div className="flex items-center gap-3 px-3 pb-3 lg:hidden">
        <ModeToggle mode={mode} onModeChange={onModeChange} />
        <div className="flex-1">
          <SearchBar scope={nodeId} />
        </div>
      </div>
    </header>
  );
}
