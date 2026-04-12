// browse API の TanStack Query オプション定義

import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
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

// キーワード検索
export function searchOptions(query: string, kind?: string) {
  const params = new URLSearchParams({ q: query, limit: "50" });
  if (kind) params.set("kind", kind);
  return queryOptions({
    queryKey: ["search", query, kind],
    queryFn: () => apiFetch<SearchResponse>(`/api/search?${params}`),
    enabled: query.length >= 2,
  });
}
