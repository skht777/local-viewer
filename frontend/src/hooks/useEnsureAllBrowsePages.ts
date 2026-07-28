// ビューワー表示中に兄弟一覧の全ページをキャッシュへ充填する
// - リロード/ディープリンク/サムネイル直クリック起動は fetchAllBrowsePages を
//   通らないため、infinite query の 1 ページ目 (100 件) で打ち切られる。
//   ビューワー表示中は FileBrowser (無限スクロールの発火主体) がアンマウント
//   されるので、ここで能動的に残りページを取得する
// - enabled (=ビューワー表示中) かつ次ページが残っている場合のみ実行
// - 取得失敗時は例外を握り潰し、取得済みページのみで表示を継続する

import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { fetchAllBrowsePages } from "./api/browseQueries";
import type { SortOrder } from "./useViewerParams";

interface UseEnsureAllBrowsePagesParams {
  enabled: boolean;
  nodeId: string | undefined;
  sort: SortOrder;
  hasNextPage: boolean;
}

export function useEnsureAllBrowsePages({
  enabled,
  nodeId,
  sort,
  hasNextPage,
}: UseEnsureAllBrowsePagesParams): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!enabled || !hasNextPage || !nodeId) {
      return;
    }
    const targetNodeId = nodeId;

    async function ensureAllPages() {
      try {
        await fetchAllBrowsePages(queryClient, targetNodeId, sort);
      } catch {
        // 失敗時は取得済みページのみで表示を継続する（infinite query 側の再取得に委ねる）
      }
    }
    ensureAllPages();
  }, [enabled, nodeId, sort, hasNextPage, queryClient]);
}
