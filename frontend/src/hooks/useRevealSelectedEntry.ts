// `?select={node_id}` の対象エントリを可視領域へ運ぶフック
// - ビューワー閉鎖時の起点復帰や検索結果からの遷移で付与される select 対象は、
//   1 ページ目 (100 件) の外にあると読み込まれず選択もスクロールも起きない
// - 読み込み済みなら仮想スクロールで該当位置へスクロールする (対象ごとに 1 回だけ)
// - 未ロードなら hasMore の限り追加ページを取得して探し続ける
// - 対象が存在しない (別タブのエントリ等) 場合は上限到達 or hasMore=false で静かに終了する

import { useEffect, useRef } from "react";

interface UseRevealSelectedEntryParams {
  selectedNodeId?: string;
  // 表示中エントリの node_id → index (FileBrowser のフィルタ後配列基準)
  indexMap: Map<string, number>;
  scrollToItem: (index: number) => void;
  hasMore?: boolean;
  isLoadingMore?: boolean;
  onLoadMore?: () => void;
}

// 暴走防止: 1 つの select 対象あたりの追加取得回数上限 (= 最大 20,000 エントリ)
const MAX_AUTO_FETCH_PAGES = 200;

export function useRevealSelectedEntry({
  selectedNodeId,
  indexMap,
  scrollToItem,
  hasMore,
  isLoadingMore,
  onLoadMore,
}: UseRevealSelectedEntryParams): void {
  // 現在追跡中の select 対象 (変わったら探索状態をリセットする)
  const trackedIdRef = useRef<string | null>(null);
  const scrolledIdRef = useRef<string | null>(null);
  const fetchCountRef = useRef(0);

  useEffect(() => {
    if (!selectedNodeId) {
      trackedIdRef.current = null;
      return;
    }
    if (trackedIdRef.current !== selectedNodeId) {
      trackedIdRef.current = selectedNodeId;
      scrolledIdRef.current = null;
      fetchCountRef.current = 0;
    }

    const index = indexMap.get(selectedNodeId);
    if (index !== undefined) {
      if (scrolledIdRef.current !== selectedNodeId) {
        scrolledIdRef.current = selectedNodeId;
        scrollToItem(index);
      }
      return;
    }

    // 未ロード: 次ページを取得して探索を続ける
    if (!hasMore || isLoadingMore || !onLoadMore) {
      return;
    }
    if (fetchCountRef.current >= MAX_AUTO_FETCH_PAGES) {
      console.warn(
        `useRevealSelectedEntry: 追加取得が上限 ${MAX_AUTO_FETCH_PAGES} ページに達しました`,
      );
      return;
    }
    fetchCountRef.current += 1;
    onLoadMore();
  }, [selectedNodeId, indexMap, scrollToItem, hasMore, isLoadingMore, onLoadMore]);
}
