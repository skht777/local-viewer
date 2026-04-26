// マンガモード用の縦スクロール仮想化フック
// - estimateSize: pageSizes が指定されたら正確な比率、なければ 3:4 推定
// - 初期 scrollToIndex は initialIndex > 0 のときに 1 度だけ
// - pageSizesReady === true で virtualizer.measure() を再実行
// - zoom 変動時にアンカー（anchorIndexRef.current）位置を維持
//
// MangaViewer / PdfMangaViewer で共有。pageSizes は PDF 専用、画像系は省略

import type { Virtualizer } from "@tanstack/react-virtual";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, useState } from "react";

interface PageSize {
  width: number;
  height: number;
}

interface UseMangaVirtualizerParams {
  count: number;
  zoomLevel: number;
  // 0-based, applied once at first render when count > 0
  initialIndex: number;
  pageSizes?: PageSize[];
  pageSizesReady?: boolean;
  // 現在の表示 index を保持する ref。zoom 変更時のアンカー維持に使う。
  // mangaScroll は virtualizer 後に作られるため、ref 経由で参照する。
  // 未指定なら zoom 変更でアンカー維持は行われない
  anchorIndexRef?: React.RefObject<number>;
}

interface UseMangaVirtualizerResult {
  virtualizer: Virtualizer<HTMLDivElement, Element>;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  scrollElement: HTMLDivElement | null;
}

const DEFAULT_CONTAINER_WIDTH = 800;
const DEFAULT_ASPECT_HEIGHT = 4;
const DEFAULT_ASPECT_WIDTH = 3;

export function useMangaVirtualizer({
  count,
  zoomLevel,
  initialIndex,
  pageSizes,
  pageSizesReady,
  anchorIndexRef,
}: UseMangaVirtualizerParams): UseMangaVirtualizerResult {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);

  useEffect(() => {
    setScrollElement(scrollRef.current);
  }, []);

  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => {
      const containerWidth = scrollRef.current?.clientWidth ?? DEFAULT_CONTAINER_WIDTH;
      const w = (containerWidth * zoomLevel) / 100;
      // PDF: ページサイズが取得済みなら正確な高さを返す
      if (pageSizes && pageSizesReady && pageSizes[index]) {
        const ps = pageSizes[index];
        return w * (ps.height / ps.width);
      }
      // フォールバック: 3:4 比率
      return (w * DEFAULT_ASPECT_HEIGHT) / DEFAULT_ASPECT_WIDTH;
    },
    overscan: 3,
  });

  // pageSizes 取得完了時に measure を再実行（PDF）
  useEffect(() => {
    if (pageSizesReady) {
      virtualizer.measure();
    }
  }, [pageSizesReady, virtualizer]);

  // 初期表示で initialIndex の位置にスクロール（モード切替時の位置引き継ぎ）
  const initialScrollDone = useRef(false);
  useEffect(() => {
    if (!initialScrollDone.current && initialIndex > 0 && count > 0) {
      virtualizer.scrollToIndex(initialIndex, { align: "start" });
      initialScrollDone.current = true;
    }
  }, [initialIndex, count, virtualizer]);

  // ズーム変更時: スクロールアンカー維持（anchorIndexRef 指定時のみ）
  const prevZoomLevel = useRef(zoomLevel);
  useEffect(() => {
    if (prevZoomLevel.current === zoomLevel) {
      return;
    }
    prevZoomLevel.current = zoomLevel;
    if (!anchorIndexRef) {
      return;
    }
    const anchor = anchorIndexRef.current;
    virtualizer.measure();
    // 次フレームで scrollToIndex（measure 反映後）
    requestAnimationFrame(() => {
      virtualizer.scrollToIndex(anchor, { align: "start" });
    });
  }, [zoomLevel, virtualizer, anchorIndexRef]);

  return { virtualizer, scrollRef, scrollElement };
}
