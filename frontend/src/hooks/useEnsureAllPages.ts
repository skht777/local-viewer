// ビューワー表示中に infinite query の残りページを能動取得する汎用フック
// - ビューワー表示中は FileBrowser (無限スクロールの発火主体) がアンマウントされ、
//   ページ送りが読み込み済みページで打ち切られるため、代わりにここで取得を進める
// - fetchNextPage 完了 → isFetchingNextPage の変化で effect が再発火し、
//   hasNextPage が false になるまで連鎖する
// - 暴走防止: enabled の 1 期間中の自動取得は MAX_AUTO_FETCH_PAGES 回まで

import { useEffect, useRef } from "react";

interface UseEnsureAllPagesParams {
  enabled: boolean;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => void;
}

const MAX_AUTO_FETCH_PAGES = 200;

export function useEnsureAllPages({
  enabled,
  hasNextPage,
  isFetchingNextPage,
  fetchNextPage,
}: UseEnsureAllPagesParams): void {
  const autoFetchCountRef = useRef(0);

  useEffect(() => {
    if (!enabled) {
      autoFetchCountRef.current = 0;
      return;
    }
    if (!hasNextPage || isFetchingNextPage) {
      return;
    }
    if (autoFetchCountRef.current >= MAX_AUTO_FETCH_PAGES) {
      // 上限到達は黙殺しない（以降のページはビューワーに渡らない）
      console.warn(`useEnsureAllPages: 自動取得が上限 ${MAX_AUTO_FETCH_PAGES} ページに達しました`);
      return;
    }
    autoFetchCountRef.current += 1;
    fetchNextPage();
  }, [enabled, hasNextPage, isFetchingNextPage, fetchNextPage]);
}
