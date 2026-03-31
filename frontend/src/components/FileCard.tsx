// ファイル/フォルダ1件を表示するカード
// - kind === "image" の場合は /api/file/{node_id} で実画像プレビュー
// - その他の kind はアイコン表示

import type { Ref } from "react";
import { useState } from "react";
import type { BrowseEntry } from "../types/api";
import { formatFileSize } from "../utils/format";

interface FileCardProps {
  entry: BrowseEntry;
  onClick: (entry: BrowseEntry) => void;
  isSelected?: boolean;
  ref?: Ref<HTMLButtonElement>;
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

export function FileCard({ entry, onClick, isSelected, ref }: FileCardProps) {
  const [hasImageError, setHasImageError] = useState(false);
  const isImagePreview = entry.kind === "image" && !hasImageError;

  return (
    <button
      ref={ref}
      type="button"
      data-testid={`file-card-${entry.node_id}`}
      aria-current={isSelected ? "true" : undefined}
      onClick={() => onClick(entry)}
      className={`flex cursor-pointer flex-col overflow-hidden rounded-lg transition-all duration-150 ${isSelected ? "bg-blue-600/30 ring-2 ring-blue-500" : "bg-surface-card ring-1 ring-white/5 hover:bg-surface-raised hover:scale-[1.02]"}`}
    >
      <div className="flex aspect-square items-center justify-center bg-surface-raised text-4xl">
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
          <span className="text-xs font-mono tabular-nums text-gray-400">
            {formatFileSize(entry.size_bytes)}
          </span>
        )}
        {entry.child_count != null && (
          <span className="text-xs font-mono tabular-nums text-gray-400">
            {entry.child_count} items
          </span>
        )}
      </div>
    </button>
  );
}
