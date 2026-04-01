// ファイル一覧をサムネイルグリッドで表示するメインエリア
// - シングルクリック: カード選択（ハイライト + オーバーレイ表示）
// - ダブルクリック: アクション実行（進入/ビューワー起動）
// - sort に応じてエントリをソート（name-asc/desc, date-asc/desc）
// - tab に応じてエントリをフィルタ

import type { KeyboardEvent } from "react";
import { useEffect, useRef, useState } from "react";
import type { SortOrder, ViewerTab } from "../hooks/useViewerParams";
import type { BrowseEntry } from "../types/api";
import { FileCard } from "./FileCard";

interface FileBrowserProps {
  entries: BrowseEntry[];
  isLoading: boolean;
  onNavigate: (nodeId: string, options?: { tab?: ViewerTab }) => void;
  onImageClick?: (imageIndex: number) => void;
  onPdfClick?: (nodeId: string) => void;
  onOpenViewer?: (nodeId: string) => void;
  tab: ViewerTab;
  sort: SortOrder;
  selectedNodeId?: string;
}

// ソートキーと方向に応じてエントリを並び替え
// - name-asc: API のデフォルト順（ディレクトリ優先 + 名前昇順）をそのまま使用
// - name-desc: 名前降順（ディレクトリ優先は維持）
// - date-desc: 更新日時降順（最新が先頭）、null は末尾
// - date-asc: 更新日時昇順（最古が先頭）、null は末尾
function sortEntries(entries: BrowseEntry[], sort: SortOrder): BrowseEntry[] {
  if (sort === "name-asc") return entries;

  return [...entries].sort((a, b) => {
    if (sort === "name-desc") {
      // ディレクトリ優先は維持しつつ、名前は降順
      const aIsDir = a.kind === "directory" ? 0 : 1;
      const bIsDir = b.kind === "directory" ? 0 : 1;
      if (aIsDir !== bIsDir) return aIsDir - bIsDir;
      return b.name.localeCompare(a.name, undefined, { numeric: true, sensitivity: "base" });
    }

    // date ソート: null は末尾
    if (a.modified_at == null && b.modified_at == null) return 0;
    if (a.modified_at == null) return 1;
    if (b.modified_at == null) return -1;

    return sort === "date-desc" ? b.modified_at - a.modified_at : a.modified_at - b.modified_at;
  });
}

// タブに応じて表示する kind をフィルタ
// filesets: name ソート時は archive/PDF を先、directory を後にサブソート
function filterByTab(entries: BrowseEntry[], tab: ViewerTab, sort: SortOrder): BrowseEntry[] {
  switch (tab) {
    case "filesets": {
      const filesets = entries.filter(
        (e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf",
      );
      // date ソート時はソート済み順序を尊重し、サブソートをスキップ
      if (sort.startsWith("date")) return filesets;
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
  onOpenViewer,
  tab,
  sort,
  selectedNodeId,
}: FileBrowserProps) {
  const sorted = sortEntries(entries, sort);
  const filtered = filterByTab(sorted, tab, sort);

  // エントリ変更時（ナビゲーション・タブ切替）に先頭カードへ focus
  const firstCardRef = useRef<HTMLDivElement>(null);
  const firstEntryId = filtered[0]?.node_id ?? null;

  // ローカル選択状態（クリック選択が優先、なければ URL ?select= or 先頭カード）
  const [localSelectedId, setLocalSelectedId] = useState<string | null>(null);
  const effectiveSelectedId = localSelectedId ?? selectedNodeId ?? firstEntryId;

  // entries 変更時にローカル選択をリセット
  useEffect(() => {
    setLocalSelectedId(null);
  }, [firstEntryId]);

  useEffect(() => {
    if (firstEntryId) {
      firstCardRef.current?.focus();
    }
  }, [firstEntryId]);

  // シングルクリック: カード選択
  const handleSelect = (entry: BrowseEntry) => {
    setLocalSelectedId(entry.node_id);
  };

  // ダブルクリック / Enter: アクション実行（進入/ビューワー起動）
  const handleAction = (entry: BrowseEntry) => {
    if (entry.kind === "archive") {
      onNavigate(entry.node_id, { tab: "images" });
    } else if (entry.kind === "directory") {
      onNavigate(entry.node_id);
    } else if (entry.kind === "pdf") {
      onPdfClick?.(entry.node_id);
    } else if (entry.kind === "image" && onImageClick) {
      const imageIndex = filtered.findIndex((e) => e.node_id === entry.node_id);
      if (imageIndex >= 0) onImageClick(imageIndex);
    }
  };

  // オーバーレイ「▶ 開く」: kind に応じて適切なアクションを呼び分け
  const handleOpen = (entry: BrowseEntry) => {
    if (entry.kind === "directory" || entry.kind === "archive") {
      onOpenViewer?.(entry.node_id);
    } else if (entry.kind === "image" && onImageClick) {
      const imageIndex = filtered.findIndex((e) => e.node_id === entry.node_id);
      if (imageIndex >= 0) onImageClick(imageIndex);
    } else if (entry.kind === "pdf") {
      onPdfClick?.(entry.node_id);
    }
  };

  // オーバーレイ「→ 進入」: directory/archive のナビゲーション
  const handleEnter = (entry: BrowseEntry) => {
    if (entry.kind === "archive") {
      onNavigate(entry.node_id, { tab: "images" });
    } else if (entry.kind === "directory") {
      onNavigate(entry.node_id);
    }
  };

  // Escape で選択解除
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      setLocalSelectedId(null);
    }
  };

  // カード外クリックで選択解除
  const handleMainClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      setLocalSelectedId(null);
    }
  };

  // entry の kind に応じてオーバーレイの onOpen / onEnter コールバックを決定
  const getOpenHandler = (entry: BrowseEntry) => {
    if (
      entry.kind === "directory" ||
      entry.kind === "archive" ||
      entry.kind === "image" ||
      entry.kind === "pdf"
    ) {
      return handleOpen;
    }
    return undefined;
  };

  const getEnterHandler = (entry: BrowseEntry) => {
    if (entry.kind === "directory" || entry.kind === "archive") {
      return handleEnter;
    }
    return undefined;
  };

  return (
    <main
      className="flex-1 overflow-y-auto p-4"
      onClick={handleMainClick}
      onKeyDown={handleKeyDown}
    >
      {isLoading && <p className="text-gray-400">読み込み中...</p>}

      {!isLoading && filtered.length === 0 && (
        <div className="flex flex-col items-center gap-2 py-8">
          <p className="text-gray-500">ファイルがありません</p>
        </div>
      )}

      {!isLoading && filtered.length > 0 && (
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
          {filtered.map((entry, index) => (
            <FileCard
              key={entry.node_id}
              ref={index === 0 ? firstCardRef : undefined}
              entry={entry}
              onSelect={handleSelect}
              onDoubleClick={handleAction}
              onOpen={getOpenHandler(entry)}
              onEnter={getEnterHandler(entry)}
              isSelected={entry.node_id === effectiveSelectedId}
            />
          ))}
        </div>
      )}
    </main>
  );
}
