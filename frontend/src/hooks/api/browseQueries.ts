// browse API の TanStack Query オプション定義

import { queryOptions } from "@tanstack/react-query";
import type { BrowseResponse, SearchResponse } from "../../types/api";
import { apiFetch } from "./apiClient";

// ルート一覧 (マウントポイント) を取得
// staleTime / gcTime は App.tsx のグローバル設定を継承
export function browseRootOptions() {
  return queryOptions({
    queryKey: ["browse"],
    queryFn: () => apiFetch<BrowseResponse>("/api/browse"),
  });
}

// 特定ディレクトリの中身を取得
export function browseNodeOptions(nodeId: string | undefined) {
  return queryOptions({
    queryKey: ["browse", nodeId],
    queryFn: () => apiFetch<BrowseResponse>(`/api/browse/${nodeId}`),
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
