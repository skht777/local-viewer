// ファイルブラウザーのタブバー
// - ファイルセット / 画像 / 動画 の3タブ
// - アクティブタブをハイライト表示
// - ソートトグル（名前/更新日 + asc/desc）

import type { SortOrder, ViewerTab } from "../hooks/useViewerParams";

interface ViewerTabsProps {
  activeTab: ViewerTab;
  onTabChange: (tab: ViewerTab) => void;
  disabledTabs?: Set<ViewerTab>;
  sort?: SortOrder;
  onSortChange?: (sort: SortOrder) => void;
}

const TABS: { key: ViewerTab; label: string }[] = [
  { key: "filesets", label: "ファイルセット" },
  { key: "images", label: "画像" },
  { key: "videos", label: "動画" },
];

// ソートキーのデフォルト方向
const SORT_DEFAULTS: Record<string, SortOrder> = {
  name: "name-asc",
  date: "date-desc",
};

// ソートキーの反転マップ
const SORT_FLIP: Record<SortOrder, SortOrder> = {
  "name-asc": "name-desc",
  "name-desc": "name-asc",
  "date-asc": "date-desc",
  "date-desc": "date-asc",
};

export function ViewerTabs({
  activeTab,
  onTabChange,
  disabledTabs,
  sort = "name-asc",
  onSortChange,
}: ViewerTabsProps) {
  const activeKey = sort.startsWith("name") ? "name" : "date";
  const isAsc = sort.endsWith("asc");

  // クリック時: アクティブキー再クリック → 反転、別キー → デフォルト方向
  const handleSortClick = (key: "name" | "date") => {
    if (!onSortChange) {
      return;
    }
    if (key === activeKey) {
      onSortChange(SORT_FLIP[sort]);
    } else {
      onSortChange(SORT_DEFAULTS[key]);
    }
  };

  return (
    <nav className="flex items-center border-b border-white/5 bg-surface-card px-4">
      {TABS.map((tab) => {
        const isDisabled = disabledTabs?.has(tab.key) ?? false;
        return (
          <button
            key={tab.key}
            type="button"
            data-testid={`tab-${tab.key}`}
            disabled={isDisabled}
            onClick={() => onTabChange(tab.key)}
            className={`px-4 py-2 text-sm font-medium transition-colors ${
              isDisabled
                ? "cursor-not-allowed text-gray-600"
                : activeTab === tab.key
                  ? "border-b-2 border-blue-500 text-white"
                  : "text-gray-400 hover:text-gray-200"
            }`}
          >
            {tab.label}
          </button>
        );
      })}

      {onSortChange && (
        <>
          <div className="ml-auto" />
          <div role="group" aria-label="並び替え" className="flex rounded-lg bg-surface-base">
            <button
              type="button"
              data-testid="sort-name"
              onClick={() => handleSortClick("name")}
              className={`rounded-l-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                activeKey === "name"
                  ? "bg-blue-600 text-white"
                  : "text-gray-400 hover:bg-surface-raised hover:text-gray-200"
              }`}
              aria-pressed={activeKey === "name"}
            >
              名前 {activeKey === "name" ? (isAsc ? "↑" : "↓") : ""}
            </button>
            <button
              type="button"
              data-testid="sort-date"
              onClick={() => handleSortClick("date")}
              className={`rounded-r-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                activeKey === "date"
                  ? "bg-blue-600 text-white"
                  : "text-gray-400 hover:bg-surface-raised hover:text-gray-200"
              }`}
              aria-pressed={activeKey === "date"}
            >
              更新日 {activeKey === "date" ? (isAsc ? "↑" : "↓") : ""}
            </button>
          </div>
        </>
      )}
    </nav>
  );
}
