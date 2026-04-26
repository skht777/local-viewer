// FileBrowser のグリッド描画専用コンポーネント
// - virtualizer.getVirtualItems().length > 0 のとき仮想スクロール経路（動的高さ）
// - 0 のとき非仮想 fallback（grid-cols-* レスポンシブ）
// - 先頭カードに firstCardRef を付与してオートフォーカスを有効化

import type { useVirtualGrid } from "../hooks/useVirtualGrid";
import type { BrowseEntry } from "../types/api";
import { FileCard } from "./FileCard";

type VirtualGridResult = ReturnType<typeof useVirtualGrid>;

type EntryHandler = (entry: BrowseEntry) => void;

interface FileBrowserGridProps {
  filtered: BrowseEntry[];
  effectiveSelectedId: string | null;
  batchThumbnails: Map<string, string>;
  firstCardRef: React.RefObject<HTMLDivElement | null>;
  virtualizer: VirtualGridResult["virtualizer"];
  columns: number;
  getRowItems: VirtualGridResult["getRowItems"];
  measureElement: VirtualGridResult["measureElement"];
  onSelect: EntryHandler;
  onDoubleClick: EntryHandler;
  getOpenHandler: (entry: BrowseEntry) => EntryHandler | undefined;
  getEnterHandler: (entry: BrowseEntry) => EntryHandler | undefined;
}

export function FileBrowserGrid({
  filtered,
  effectiveSelectedId,
  batchThumbnails,
  firstCardRef,
  virtualizer,
  columns,
  getRowItems,
  measureElement,
  onSelect,
  onDoubleClick,
  getOpenHandler,
  getEnterHandler,
}: FileBrowserGridProps) {
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
                      onSelect={onSelect}
                      onDoubleClick={onDoubleClick}
                      onOpen={getOpenHandler(entry)}
                      onEnter={getEnterHandler(entry)}
                      isSelected={entry.node_id === effectiveSelectedId}
                      batchThumbnailUrl={batchThumbnails.get(entry.node_id)}
                      batchThumbnails={batchThumbnails}
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
          onSelect={onSelect}
          onDoubleClick={onDoubleClick}
          onOpen={getOpenHandler(entry)}
          onEnter={getEnterHandler(entry)}
          isSelected={entry.node_id === effectiveSelectedId}
          batchThumbnailUrl={batchThumbnails.get(entry.node_id)}
          batchThumbnails={batchThumbnails}
        />
      ))}
    </div>
  );
}
