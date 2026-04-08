// ファイル一覧をサムネイルグリッドで表示するメインエリア
// - シングルクリック: カード選択（ハイライト + オーバーレイ表示）
// - ダブルクリック: アクション実行（進入/ビューワー起動）
// - キーボード: 矢印/WASDでグリッド移動、g/Enter進入、Space開く等
// - sort に応じてエントリをソート（name-asc/desc, date-asc/desc）
// - tab に応じてエントリをフィルタ

import type { KeyboardEvent } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useBatchThumbnails } from "../hooks/api/thumbnailQueries";
import { useBrowseKeyboard } from "../hooks/useBrowseKeyboard";
import { useVirtualGrid } from "../hooks/useVirtualGrid";
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
  onLoadMore,
}: FileBrowserProps) {
  // サーバーサイドソート済みのため sortEntries はスキップ
  const filtered = useMemo(() => filterByTab(entries, tab, sort), [entries, tab, sort]);

  // バッチサムネイル: カードサムネイル + ディレクトリプレビューの node_ids を収集
  // 本体カードを先にし、バッチ API の件数上限でも優先的に取得されるようにする
  const thumbnailNodeIds = useMemo(() => {
    const ids: string[] = [];
    for (const e of filtered) {
      if (e.kind === "image" || e.kind === "archive" || e.kind === "video") {
        ids.push(e.node_id);
      }
    }
    // ディレクトリのプレビュー画像 node_ids を後ろに追加
    for (const e of filtered) {
      if (e.kind === "directory" && e.preview_node_ids) {
        for (const pid of e.preview_node_ids) {
          ids.push(pid);
        }
      }
    }
    return ids;
  }, [filtered]);
  // 無限スクロール: センチネル要素の IntersectionObserver
  const sentinelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!hasMore || !onLoadMore) return;
    const el = sentinelRef.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && !isLoadingMore) {
          onLoadMore();
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [hasMore, isLoadingMore, onLoadMore]);

  // 仮想グリッド
  const {
    scrollRef,
    virtualizer,
    columns,
    getRowItems,
    scrollToItem,
    getColumnCount,
    measureElement,
  } = useVirtualGrid({
    itemCount: filtered.length,
    enabled: !isLoading && filtered.length > 0,
  });

  const { thumbnails: batchThumbnails, isLoading: isBatchLoading } =
    useBatchThumbnails(thumbnailNodeIds);

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
  const handleSelect = useCallback((entry: BrowseEntry) => {
    setLocalSelectedId(entry.node_id);
  }, []);

  // ダブルクリック / Enter / g: アクション実行（進入/ビューワー起動）
  const handleAction = useCallback(
    (entry: BrowseEntry) => {
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
    },
    [filtered, onNavigate, onPdfClick, onImageClick],
  );

  // オーバーレイ「▶ 開く」/ Space: kind に応じて適切なアクションを呼び分け
  const handleOpen = useCallback(
    (entry: BrowseEntry) => {
      if (entry.kind === "directory" || entry.kind === "archive") {
        onOpenViewer?.(entry.node_id);
      } else if (entry.kind === "image" && onImageClick) {
        const imageIndex = filtered.findIndex((e) => e.node_id === entry.node_id);
        if (imageIndex >= 0) onImageClick(imageIndex);
      } else if (entry.kind === "pdf") {
        onPdfClick?.(entry.node_id);
      }
    },
    [filtered, onOpenViewer, onImageClick, onPdfClick],
  );

  // オーバーレイ「→ 進入」: directory/archive のナビゲーション
  const handleEnter = useCallback(
    (entry: BrowseEntry) => {
      if (entry.kind === "archive") {
        onNavigate(entry.node_id, { tab: "images" });
      } else if (entry.kind === "directory") {
        onNavigate(entry.node_id);
      }
    },
    [onNavigate],
  );

  // キーボード移動: delta 分だけ選択を移動し、仮想スクロールで可視化
  const handleMove = useCallback(
    (delta: number) => {
      const currentIndex = filtered.findIndex((e) => e.node_id === effectiveSelectedId);
      const newIndex = currentIndex + delta;
      if (newIndex < 0 || newIndex >= filtered.length) return;
      const target = filtered[newIndex];
      setLocalSelectedId(target.node_id);
      // 仮想スクロールで対象行を可視領域に移動
      scrollToItem(newIndex);
      // DOM 更新後にフォーカスを移動
      requestAnimationFrame(() => {
        const el = document.querySelector<HTMLElement>(
          `[data-testid="file-card-${target.node_id}"]`,
        );
        el?.focus({ preventScroll: true });
      });
    },
    [filtered, effectiveSelectedId, scrollToItem],
  );

  useBrowseKeyboard(
    {
      move: handleMove,
      action: () => {
        const entry = filtered.find((e) => e.node_id === effectiveSelectedId);
        if (entry) handleAction(entry);
      },
      open: () => {
        const entry = filtered.find((e) => e.node_id === effectiveSelectedId);
        if (entry) handleOpen(entry);
      },
      goParent: onGoParent ?? (() => {}),
      focusTree: onFocusTree ?? (() => {}),
      toggleMode: onToggleMode ?? (() => {}),
      sortName: onSortName ?? (() => {}),
      sortDate: onSortDate ?? (() => {}),
      tabChange: onTabChange ?? (() => {}),
      getColumnCount,
    },
    keyboardEnabled,
  );

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

      {!isLoading &&
        filtered.length > 0 &&
        (() => {
          const virtualItems = virtualizer.getVirtualItems();
          // 仮想化が有効（スクロールコンテナにサイズがある場合）
          if (virtualItems.length > 0) {
            return (
              /* 仮想スクロールの動的値は Tailwind で表現不可のためインラインスタイル使用 */
              <div style={{ height: virtualizer.getTotalSize() }} className="relative">
                {virtualItems.map((virtualRow) => {
                  const { start, end } = getRowItems(virtualRow.index);
                  return (
                    <div
                      key={virtualRow.key}
                      data-index={virtualRow.index}
                      ref={measureElement}
                      style={{
                        position: "absolute",
                        top: 0,
                        transform: `translateY(${virtualRow.start}px)`,
                        width: "100%",
                      }}
                    >
                      <div
                        className="grid gap-4"
                        style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
                      >
                        {filtered.slice(start, end).map((entry, i) => {
                          const itemIndex = start + i;
                          return (
                            <FileCard
                              key={entry.node_id}
                              ref={itemIndex === 0 ? firstCardRef : undefined}
                              entry={entry}
                              onSelect={handleSelect}
                              onDoubleClick={handleAction}
                              onOpen={getOpenHandler(entry)}
                              onEnter={getEnterHandler(entry)}
                              isSelected={entry.node_id === effectiveSelectedId}
                              batchThumbnailUrl={batchThumbnails.get(entry.node_id)}
                              batchThumbnails={batchThumbnails}
                              isBatchLoading={isBatchLoading}
                            />
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
              </div>
            );
          }
          // フォールバック: 仮想化が無効 (テスト環境等スクロールコンテナのサイズが不明)
          return (
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
                  batchThumbnailUrl={batchThumbnails.get(entry.node_id)}
                  batchThumbnails={batchThumbnails}
                />
              ))}
            </div>
          );
        })()}

      {/* 無限スクロール: センチネル + ローディング表示 */}
      {hasMore && (
        <div ref={sentinelRef} className="flex justify-center py-4">
          {isLoadingMore && <p className="text-gray-400">読み込み中...</p>}
        </div>
      )}
    </main>
  );
}
