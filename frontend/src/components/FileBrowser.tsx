// ファイル一覧をサムネイルグリッドで表示するメインエリア
// - シングルクリック: カード選択（ハイライト + オーバーレイ表示）
// - ダブルクリック: アクション実行（進入/ビューワー起動）
// - キーボード: 矢印/WASDでグリッド移動、g/Enter進入、Space開く等
// - sort に応じてエントリをソート（name-asc/desc, date-asc/desc）
// - tab に応じてエントリをフィルタ
// - 選択 / アクション / キーボード / グリッド描画は分割した hooks / 子コンポーネントへ委譲

import { useMemo } from "react";
import { useBatchThumbnails } from "../hooks/api/thumbnailQueries";
import { useFileBrowserActions } from "../hooks/useFileBrowserActions";
import { useFileBrowserInfiniteScroll } from "../hooks/useFileBrowserInfiniteScroll";
import { useFileBrowserKeyboardBindings } from "../hooks/useFileBrowserKeyboardBindings";
import { useFileBrowserSelection } from "../hooks/useFileBrowserSelection";
import { useVirtualGrid } from "../hooks/useVirtualGrid";
import type { SortOrder, ViewerTab } from "../hooks/useViewerParams";
import type { BrowseEntry } from "../types/api";
import { FileBrowserGrid } from "./FileBrowserGrid";

interface FileBrowserProps {
  entries: BrowseEntry[];
  isLoading: boolean;
  onNavigate: (nodeId: string, options?: { tab?: ViewerTab }) => void;
  onImageClick?: (imageIndex: number) => void;
  onPdfClick?: (nodeId: string) => void;
  onOpenViewer?: (nodeId: string) => void;
  onGoParent?: () => void;
  onTabChange?: (tab: ViewerTab) => void;
  onFocusTree?: () => void;
  onToggleMode?: () => void;
  onSortName?: () => void;
  onSortDate?: () => void;
  tab: ViewerTab;
  sort: SortOrder;
  selectedNodeId?: string;
  keyboardEnabled?: boolean;
  hasMore?: boolean;
  isLoadingMore?: boolean;
  isError?: boolean;
  onLoadMore?: () => void;
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
      if (sort.startsWith("date")) {
        return filesets;
      }
      return filesets.toSorted((a, b) => {
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

// バッチサムネイル対象 node_id 群を生成
// 本体カード（image/archive/video）→ ディレクトリのプレビュー画像 の順で並べ、
// バッチ API の上限に達しても本体側を優先して取得する
function buildThumbnailNodeIds(filtered: BrowseEntry[]): string[] {
  const ids: string[] = [];
  for (const e of filtered) {
    if (e.kind === "image" || e.kind === "archive" || e.kind === "video") {
      ids.push(e.node_id);
    }
  }
  for (const e of filtered) {
    if (e.kind === "directory" && e.preview_node_ids) {
      for (const pid of e.preview_node_ids) {
        ids.push(pid);
      }
    }
  }
  return ids;
}

export function FileBrowser({
  entries,
  isLoading,
  onNavigate,
  onImageClick,
  onPdfClick,
  onOpenViewer,
  onGoParent,
  onTabChange,
  onFocusTree,
  onToggleMode,
  onSortName,
  onSortDate,
  tab,
  sort,
  selectedNodeId,
  keyboardEnabled = true,
  hasMore,
  isLoadingMore,
  isError,
  onLoadMore,
}: FileBrowserProps) {
  // サーバーサイドソート済みのため sortEntries はスキップ
  const filtered = useMemo(() => filterByTab(entries, tab, sort), [entries, tab, sort]);

  // node_id → index の O(1) ルックアップマップ (findIndex O(n) を回避)
  const indexMap = useMemo(() => {
    const map = new Map<string, number>();
    filtered.forEach((e, idx) => map.set(e.node_id, idx));
    return map;
  }, [filtered]);

  const thumbnailNodeIds = useMemo(() => buildThumbnailNodeIds(filtered), [filtered]);

  // 無限スクロール: センチネル要素の IntersectionObserver
  const { sentinelRef } = useFileBrowserInfiniteScroll({
    hasMore,
    isLoadingMore,
    isError,
    onLoadMore,
  });

  // 仮想グリッド
  const {
    scrollRef,
    virtualizer,
    columns,
    getRowItems,
    scrollToItem,
    getColumnCount,
    measureElement,
  } = useVirtualGrid({ itemCount: filtered.length, enabled: !isLoading && filtered.length > 0 });

  const { thumbnails: batchThumbnails } = useBatchThumbnails(thumbnailNodeIds);

  const {
    effectiveSelectedId,
    firstCardRef,
    setLocalSelectedId,
    handleSelect,
    handleKeyDown,
    handleMainClick,
  } = useFileBrowserSelection({ filtered, selectedNodeId });

  const { handleAction, getOpenHandler, getEnterHandler, handleOpen } = useFileBrowserActions({
    indexMap,
    onNavigate,
    onImageClick,
    onPdfClick,
    onOpenViewer,
  });

  useFileBrowserKeyboardBindings({
    filtered,
    indexMap,
    effectiveSelectedId,
    scrollToItem,
    setLocalSelectedId,
    handleAction,
    handleOpen,
    getColumnCount,
    keyboardEnabled,
    onGoParent,
    onFocusTree,
    onToggleMode,
    onSortName,
    onSortDate,
    onTabChange,
  });

  return (
    <main
      ref={scrollRef}
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
        <FileBrowserGrid
          filtered={filtered}
          effectiveSelectedId={effectiveSelectedId}
          batchThumbnails={batchThumbnails}
          firstCardRef={firstCardRef}
          virtualizer={virtualizer}
          columns={columns}
          getRowItems={getRowItems}
          measureElement={measureElement}
          onSelect={handleSelect}
          onDoubleClick={handleAction}
          getOpenHandler={getOpenHandler}
          getEnterHandler={getEnterHandler}
        />
      )}

      {/* 無限スクロール: センチネル + ローディング/エラー表示 */}
      {hasMore && (
        <div ref={sentinelRef} className="flex justify-center py-4">
          {isLoadingMore && <p className="text-gray-400">読み込み中...</p>}
          {isError && (
            <p className="text-red-400">読み込みに失敗しました。ページをリロードしてください。</p>
          )}
        </div>
      )}
    </main>
  );
}
