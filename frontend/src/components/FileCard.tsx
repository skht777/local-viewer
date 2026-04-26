// ファイル/フォルダ1件を表示するカード
// - シングルクリック: 選択（onSelect）
// - ダブルクリック: アクション実行（onDoubleClick）
// - 選択時: アクションオーバーレイを表示（▶ 開く / → 進入）
// - kind === "image": /api/thumbnail/{node_id} でサムネイルプレビュー
// - kind === "directory" + preview_node_ids: PreviewGrid でサムネイル表示
// - kind === "archive": /api/thumbnail/{node_id} でサムネイル表示
// - kind === "video": /api/thumbnail/{node_id} で ffmpeg フレーム抽出サムネイル
// - kind === "pdf": usePdfThumbnail で先頭ページサムネイル表示

import type { KeyboardEvent, Ref } from "react";
import { memo, useState } from "react";
import { usePdfThumbnail } from "../hooks/usePdfThumbnail";
import type { BrowseEntry } from "../types/api";
import { formatFileSize } from "../utils/format";
import { PreviewGrid } from "./PreviewGrid";

interface FileCardProps {
  entry: BrowseEntry;
  onSelect: (entry: BrowseEntry) => void;
  onDoubleClick: (entry: BrowseEntry) => void;
  onOpen?: (entry: BrowseEntry) => void;
  onEnter?: (entry: BrowseEntry) => void;
  isSelected?: boolean;
  ref?: Ref<HTMLDivElement>;
  batchThumbnailUrl?: string;
  batchThumbnails?: Map<string, string>;
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

export const FileCard = memo(function FileCard({
  entry,
  onSelect,
  onDoubleClick,
  onOpen,
  onEnter,
  isSelected,
  ref,
  batchThumbnailUrl,
  batchThumbnails,
}: FileCardProps) {
  const [hasImageError, setHasImageError] = useState(false);
  const [hasPreviewError, setHasPreviewError] = useState(false);

  // バッチ Blob URL があれば使用、なければスケルトン表示（個別 API フォールバックなし）
  const thumbSrc = batchThumbnailUrl;
  const isImagePreview = entry.kind === "image" && !hasImageError;

  // ディレクトリ: preview_node_ids があればサムネイルグリッド表示
  const hasDirectoryPreview =
    entry.kind === "directory" &&
    entry.preview_node_ids != null &&
    entry.preview_node_ids.length > 0 &&
    !hasPreviewError;

  // アーカイブ: /api/thumbnail/{node_id} でサムネイル表示
  const hasArchivePreview = entry.kind === "archive" && !hasPreviewError;

  // 動画: ffmpeg フレーム抽出でサムネイル表示
  const hasVideoPreview = entry.kind === "video" && !hasPreviewError;

  // PDF: pdfjs-dist で先頭ページをサムネイル表示
  const pdfThumbnail = usePdfThumbnail(entry.node_id, entry.kind === "pdf" && !hasPreviewError);
  const hasPdfPreview = entry.kind === "pdf" && pdfThumbnail.url != null && !hasPreviewError;

  // Enter: アクション実行（進入/ビューワー起動）
  // Space: ビューワーで開く（onOpen があれば優先、なければ onSelect にフォールバック）
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      onDoubleClick(entry);
    } else if (e.key === " ") {
      e.preventDefault();
      (onOpen ?? onSelect)(entry);
    }
  };

  return (
    <div
      ref={ref}
      role="button"
      tabIndex={0}
      data-testid={`file-card-${entry.node_id}`}
      aria-current={isSelected ? "true" : undefined}
      onClick={() => onSelect(entry)}
      onDoubleClick={() => onDoubleClick(entry)}
      onKeyDown={handleKeyDown}
      className={`flex cursor-pointer flex-col overflow-hidden rounded-lg transition-all duration-150 ${isSelected ? "bg-blue-600/30 ring-2 ring-blue-500" : "bg-surface-card ring-1 ring-white/5 hover:bg-surface-raised hover:scale-[1.02]"}`}
    >
      <div className="relative flex aspect-square items-center justify-center bg-surface-raised text-4xl">
        {isImagePreview ? (
          thumbSrc ? (
            <img
              src={thumbSrc}
              alt={entry.name}
              className="h-full w-full object-cover"
              loading="lazy"
              decoding="async"
              onError={() => setHasImageError(true)}
            />
          ) : (
            <div className="h-full w-full animate-pulse bg-surface-raised" />
          )
        ) : hasDirectoryPreview ? (
          <PreviewGrid
            previewNodeIds={entry.preview_node_ids ?? []}
            onAllError={() => setHasPreviewError(true)}
            batchThumbnails={batchThumbnails}
          />
        ) : hasArchivePreview ? (
          thumbSrc ? (
            <img
              src={thumbSrc}
              alt={entry.name}
              className="h-full w-full object-cover"
              loading="lazy"
              decoding="async"
              onError={() => setHasPreviewError(true)}
            />
          ) : (
            <div className="h-full w-full animate-pulse bg-surface-raised" />
          )
        ) : hasVideoPreview ? (
          thumbSrc ? (
            <img
              src={thumbSrc}
              alt={entry.name}
              className="h-full w-full object-cover"
              loading="lazy"
              decoding="async"
              onError={() => setHasPreviewError(true)}
            />
          ) : (
            <div className="h-full w-full animate-pulse bg-surface-raised" />
          )
        ) : hasPdfPreview ? (
          <img
            src={pdfThumbnail.url ?? ""}
            alt={entry.name}
            className="h-full w-full object-cover"
            decoding="async"
          />
        ) : (
          kindIcon(entry.kind)
        )}
        {isSelected && (onOpen || onEnter) && (
          <div
            data-testid={`action-overlay-${entry.node_id}`}
            className="absolute inset-x-0 bottom-0 flex justify-center gap-2 bg-black/70 p-2 backdrop-blur-sm"
            onClick={(e) => e.stopPropagation()}
            onDoubleClick={(e) => e.stopPropagation()}
          >
            {onOpen && (
              <button
                type="button"
                data-testid={`action-open-${entry.node_id}`}
                onClick={() => onOpen(entry)}
                className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500"
              >
                ▶ 開く
              </button>
            )}
            {onEnter && (
              <button
                type="button"
                data-testid={`action-enter-${entry.node_id}`}
                onClick={() => onEnter(entry)}
                className="rounded bg-surface-raised px-3 py-1 text-sm text-white ring-1 ring-white/10 hover:bg-surface-overlay"
              >
                → 進入
              </button>
            )}
          </div>
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
    </div>
  );
});
