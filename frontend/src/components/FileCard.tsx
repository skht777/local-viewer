// ファイル/フォルダ1件を表示するカード
// - kind === "image" の場合は /api/file/{node_id} で実画像プレビュー
// - その他の kind はアイコン表示

import { useState } from "react";
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
  const [hasImageError, setHasImageError] = useState(false);
  const isImagePreview = entry.kind === "image" && !hasImageError;

  return (
    <button
      type="button"
      onClick={() => onClick(entry)}
      className="flex cursor-pointer flex-col overflow-hidden rounded-lg bg-gray-800 transition-colors hover:bg-gray-700"
    >
      <div className="flex aspect-square items-center justify-center bg-gray-750 text-4xl">
        {isImagePreview ? (
          <img
            src={`/api/file/${entry.node_id}`}
            alt={entry.name}
            className="h-full w-full object-cover"
            loading="lazy"
            decoding="async"
            onError={() => setHasImageError(true)}
          />
        ) : (
          kindIcon(entry.kind)
        )}
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
