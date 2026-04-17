// ファイルブラウザーの無限スクロール用 IntersectionObserver
// - センチネル要素が viewport 付近に入ったら `onLoadMore` を発火
// - `hasMore` / `isLoadingMore` / `isError` で発火を抑制
// - rootMargin=200px で 1 画面手前で先読みを開始
//
// Finding 2 対応: FileBrowser.tsx から無限スクロールロジックを
// testability 向上目的で hook として切り出す。

import { useEffect, useRef } from "react";

interface UseFileBrowserInfiniteScrollOptions {
  hasMore?: boolean;
  isLoadingMore?: boolean;
  isError?: boolean;
  onLoadMore?: () => void;
  /** IntersectionObserver の rootMargin（default "200px"） */
  rootMargin?: string;
}

interface UseFileBrowserInfiniteScrollReturn {
  sentinelRef: React.RefObject<HTMLDivElement | null>;
}

export function useFileBrowserInfiniteScroll({
  hasMore,
  isLoadingMore,
  isError,
  onLoadMore,
  rootMargin = "200px",
}: UseFileBrowserInfiniteScrollOptions): UseFileBrowserInfiniteScrollReturn {
  const sentinelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!hasMore || !onLoadMore) return;
    const el = sentinelRef.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && !isLoadingMore && !isError) {
          onLoadMore();
        }
      },
      { rootMargin },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [hasMore, isLoadingMore, isError, onLoadMore, rootMargin]);

  return { sentinelRef };
}
