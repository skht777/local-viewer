// ファイル一覧をサムネイルグリッドで表示するメインエリア
// - tab に応じてエントリをフィルタ
// - filesets: ディレクトリ/アーカイブ/PDF
// - images: 画像のみ
// - videos: 動画のみ

import type { ViewerTab } from "../hooks/useViewerParams";
import type { BrowseEntry } from "../types/api";
import { FileCard } from "./FileCard";

interface FileBrowserProps {
  entries: BrowseEntry[];
  isLoading: boolean;
  onNavigate: (nodeId: string) => void;
  tab: ViewerTab;
}

// タブに応じて表示する kind をフィルタ
function filterByTab(entries: BrowseEntry[], tab: ViewerTab): BrowseEntry[] {
  switch (tab) {
    case "filesets":
      return entries.filter(
        (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
      );
    case "images":
      return entries.filter((e) => e.kind === "image");
    case "videos":
      return entries.filter((e) => e.kind === "video");
  }
}

export function FileBrowser({ entries, isLoading, onNavigate, tab }: FileBrowserProps) {
  const filtered = filterByTab(entries, tab);

  const handleClick = (entry: BrowseEntry) => {
    if (entry.kind === "directory" || entry.kind === "archive") {
      onNavigate(entry.node_id);
    }
    // 画像/動画/PDF のクリックは Phase 2 以降で実装
  };

  return (
    <main className="flex-1 overflow-y-auto p-4">
      {isLoading && <p className="text-gray-400">読み込み中...</p>}

      {!isLoading && filtered.length === 0 && <p className="text-gray-500">ファイルがありません</p>}

      {!isLoading && filtered.length > 0 && (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
          {filtered.map((entry) => (
            <FileCard key={entry.node_id} entry={entry} onClick={handleClick} />
          ))}
        </div>
      )}
    </main>
  );
}
