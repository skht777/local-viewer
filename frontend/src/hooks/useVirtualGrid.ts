// 仮想グリッドフック
// - ResizeObserver でコンテナ幅を監視し、レスポンシブ列数を計算
// - @tanstack/react-virtual の useVirtualizer で行ベースの仮想化を実行
// - FileBrowser のグリッドレイアウトを仮想スクロール化するために使用

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

// Tailwind breakpoints: md=768, lg=1024, xl=1280
// grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5
function getColumnCount(width: number): number {
  if (width >= 1280) {
    return 5;
  }
  if (width >= 1024) {
    return 4;
  }
  if (width >= 768) {
    return 3;
  }
  return 2;
}

interface UseVirtualGridOptions {
  itemCount: number;
  // 行の高さ推定 (カード + gap)
  estimateRowHeight?: number;
  overscan?: number;
  enabled?: boolean;
}

export function useVirtualGrid({
  itemCount,
  estimateRowHeight = 300,
  overscan = 3,
  enabled = true,
}: UseVirtualGridOptions) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState(2);

  // ResizeObserver でコンテナ幅を監視
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !enabled) {
      return;
    }

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width } = entry.contentRect;
        setColumns(getColumnCount(width));
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [enabled]);

  const rowCount = Math.ceil(itemCount / columns);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => estimateRowHeight,
    overscan,
    enabled,
    // Tailwind gap-4 相当（行間スペース）
    gap: 16,
  });

  // アイテムインデックスから行・列を計算
  const getRowItems = useCallback(
    (rowIndex: number) => {
      const start = rowIndex * columns;
      const end = Math.min(start + columns, itemCount);
      return { start, end };
    },
    [columns, itemCount],
  );

  // アイテムインデックスを可視領域にスクロール
  const scrollToItem = useCallback(
    (itemIndex: number) => {
      const rowIndex = Math.floor(itemIndex / columns);
      virtualizer.scrollToIndex(rowIndex, { align: "center" });
    },
    [columns, virtualizer],
  );

  return {
    scrollRef,
    virtualizer,
    columns,
    rowCount,
    getRowItems,
    scrollToItem,
    getColumnCount: useCallback(() => columns, [columns]),
    measureElement: virtualizer.measureElement,
  };
}
