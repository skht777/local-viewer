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
  onImageClick?: (imageIndex: number) => void;
  onPdfClick?: (nodeId: string) => void;
  onTabChange?: (tab: ViewerTab) => void;
  tab: ViewerTab;
}

// タブに応じて表示する kind をフィルタ
// filesets: archive/PDF を先、directory を後にソート
function filterByTab(entries: BrowseEntry[], tab: ViewerTab): BrowseEntry[] {
  switch (tab) {
    case "filesets": {
      const filesets = entries.filter(
        (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
      );
      return filesets.sort((a, b) => {
        const aIsDir = a.kind === "directory" ? 1 : 0;
        const bIsDir = b.kind === "directory" ? 1 : 0;
        return aIsDir - bIsDir;
      });
    }
    case "images":
      return entries.filter((e) => e.kind === "image");
    case "videos":
      return entries.filter((e) => e.kind === "video");
  }
}

export function FileBrowser({
  entries,
  isLoading,
  onNavigate,
  onImageClick,
  onPdfClick,
  onTabChange,
  tab,
}: FileBrowserProps) {
  const filtered = filterByTab(entries, tab);

  const handleClick = (entry: BrowseEntry) => {
    if (entry.kind === "archive") {
      // アーカイブ遷移時は画像タブに自動切替
      // (アーカイブ内は画像のみなので filesets タブでは空表示になる)
      onTabChange?.("images");
      onNavigate(entry.node_id);
    } else if (entry.kind === "directory") {
      onNavigate(entry.node_id);
    } else if (entry.kind === "pdf") {
      onPdfClick?.(entry.node_id);
    } else if (entry.kind === "image" && onImageClick) {
      // フィルタ済み画像配列内でのインデックスを算出
      const imageIndex = filtered.findIndex((e) => e.node_id === entry.node_id);
      if (imageIndex >= 0) onImageClick(imageIndex);
    }
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
