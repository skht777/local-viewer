// ファイル一覧をサムネイルグリッドで表示するメインエリア
// - entries を表示
// - フォルダクリック → onNavigate
// - ファイルクリック → 将来ビューワー遷移

import type { BrowseEntry } from "../types/api";
import { FileCard } from "./FileCard";

interface FileBrowserProps {
  entries: BrowseEntry[];
  isLoading: boolean;
  onNavigate: (nodeId: string) => void;
}

export function FileBrowser({ entries, isLoading, onNavigate }: FileBrowserProps) {
  const handleClick = (entry: BrowseEntry) => {
    if (entry.kind === "directory" || entry.kind === "archive") {
      onNavigate(entry.node_id);
    }
    // 画像/動画/PDF のクリックは Phase 2 以降で実装
  };

  return (
    <main className="flex-1 overflow-y-auto p-4">
      {isLoading && <p className="text-gray-400">読み込み中...</p>}

      {!isLoading && entries.length === 0 && <p className="text-gray-500">ファイルがありません</p>}

      {!isLoading && entries.length > 0 && (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
          {entries.map((entry) => (
            <FileCard key={entry.node_id} entry={entry} onClick={handleClick} />
          ))}
        </div>
      )}
    </main>
  );
}
