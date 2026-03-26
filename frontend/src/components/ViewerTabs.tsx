// ファイルブラウザーのタブバー
// - ファイルセット / 画像 / 動画 の3タブ
// - アクティブタブをハイライト表示

import type { ViewerTab } from "../hooks/useViewerParams";

interface ViewerTabsProps {
  activeTab: ViewerTab;
  onTabChange: (tab: ViewerTab) => void;
}

const TABS: { key: ViewerTab; label: string }[] = [
  { key: "filesets", label: "ファイルセット" },
  { key: "images", label: "画像" },
  { key: "videos", label: "動画" },
];

export function ViewerTabs({ activeTab, onTabChange }: ViewerTabsProps) {
  return (
    <nav className="flex border-b border-gray-700 bg-gray-800 px-4">
      {TABS.map((tab) => (
        <button
          key={tab.key}
          type="button"
          data-testid={`tab-${tab.key}`}
          onClick={() => onTabChange(tab.key)}
          className={`px-4 py-2 text-sm font-medium transition-colors ${
            activeTab === tab.key
              ? "border-b-2 border-blue-500 text-white"
              : "text-gray-400 hover:text-gray-200"
          }`}
        >
          {tab.label}
        </button>
      ))}
    </nav>
  );
}
