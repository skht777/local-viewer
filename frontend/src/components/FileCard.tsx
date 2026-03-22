// ファイル/フォルダ1件を表示するカード
// - kind に応じたアイコン表示
// - サムネイルは Phase 1 ではプレースホルダー

import type { BrowseEntry } from "../types/api";

interface FileCardProps {
  entry: BrowseEntry;
  onClick: (entry: BrowseEntry) => void;
}

// ファイルサイズを人間可読な形式にフォーマット
function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

// kind に応じたアイコン
function kindIcon(kind: BrowseEntry["kind"]): string {
  switch (kind) {
    case "directory":
      return "\u{1F4C1}";
    case "image":
      return "\u{1F5BC}";
    case "video":
      return "\u{1F3AC}";
    case "pdf":
      return "\u{1F4C4}";
    case "archive":
      return "\u{1F4E6}";
    default:
      return "\u{1F4C3}";
  }
}

export function FileCard({ entry, onClick }: FileCardProps) {
  return (
    <button
      type="button"
      onClick={() => onClick(entry)}
      className="flex cursor-pointer flex-col overflow-hidden rounded-lg bg-gray-800 transition-colors hover:bg-gray-700"
    >
      <div className="flex aspect-square items-center justify-center bg-gray-750 text-4xl">
        {kindIcon(entry.kind)}
      </div>
      <div className="p-2">
        <p className="truncate text-sm">{entry.name}</p>
        {entry.size_bytes != null && (
          <span className="text-xs text-gray-400">{formatFileSize(entry.size_bytes)}</span>
        )}
        {entry.child_count != null && (
          <span className="text-xs text-gray-400">{entry.child_count} items</span>
        )}
      </div>
    </button>
  );
}
