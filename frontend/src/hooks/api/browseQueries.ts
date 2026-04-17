// browse API の TanStack Query オプション定義

import { infiniteQueryOptions, queryOptions, type QueryClient } from "@tanstack/react-query";
import type { SortOrder } from "../../hooks/useViewerParams";
import type { BrowseResponse, SearchResponse } from "../../types/api";
import { apiFetch } from "./apiClient";

// 特定ディレクトリの中身を取得 (後方互換: ページネーションなし)
export function browseNodeOptions(nodeId: string | undefined, sort?: SortOrder) {
  const sortParam = sort && sort !== "name-asc" ? `?sort=${sort}` : "";
  return queryOptions({
    queryKey: ["browse", nodeId, sort ?? "name-asc"],
    queryFn: () => apiFetch<BrowseResponse>(`/api/browse/${nodeId}${sortParam}`),
    enabled: !!nodeId,
  });
}

// ページネーション対応の無限スクロールクエリ
const PAGE_SIZE = 100;

export function browseInfiniteOptions(nodeId: string | undefined, sort: SortOrder) {
  return infiniteQueryOptions({
    queryKey: ["browse-infinite", nodeId, sort],
    queryFn: async ({ pageParam }) => {
      const params = new URLSearchParams({
        limit: String(PAGE_SIZE),
        sort,
      });
      if (pageParam) params.set("cursor", pageParam);
      return apiFetch<BrowseResponse>(`/api/browse/${nodeId}?${params.toString()}`);
    },
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
    enabled: !!nodeId,
  });
}

// ビューワー起動時の全ページプリフェッチ上限（= 最大 20,000 エントリ）
// `fetchInfiniteQuery` の `pages` に渡すと、`getNextPageParam` が undefined を返した時点で
// ライブラリ側の do-while ループが自発的に break する。安全上限として機能。
export const MAX_PAGES = 200;

// browseInfiniteOptions の全ページを逐次フェッチしてキャッシュに詰める
// - ビューワー起動時に兄弟画像が 100 件で打ち切られないようにするためのヘルパー
// - `prefetchInfiniteQuery` は例外を握り潰すため、呼び元の try/catch で拾える
//   `fetchInfiniteQuery` を使う
// - 上限到達（最終ページに `next_cursor` が残っている）は警告のみで続行
export async function fetchAllBrowsePages(
  queryClient: QueryClient,
  nodeId: string,
  sort: SortOrder,
): Promise<void> {
  const options = browseInfiniteOptions(nodeId, sort);
  const result = await queryClient.fetchInfiniteQuery({
    ...options,
    pages: MAX_PAGES,
  });
  const lastPage = result.pages[result.pages.length - 1];
  if (result.pages.length >= MAX_PAGES && lastPage?.next_cursor) {
    console.warn(
      `fetchAllBrowsePages: MAX_PAGES (${MAX_PAGES}) に到達しました。nodeId=${nodeId} の続きのページは取得されていません`,
    );
  }
}

// キーワード検索
// scope: ディレクトリスコープの node_id（指定ディレクトリ配下のみ検索）
export function searchOptions(query: string, kind?: string, scope?: string) {
  const params = new URLSearchParams({ q: query, limit: "50" });
  if (kind) params.set("kind", kind);
  if (scope) params.set("scope", scope);
  return queryOptions({
    queryKey: ["search", query, kind, scope],
    queryFn: () => apiFetch<SearchResponse>(`/api/search?${params}`),
    enabled: query.length >= 2,
  });
}
