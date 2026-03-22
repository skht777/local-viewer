// browse API の TanStack Query オプション定義

import { queryOptions } from "@tanstack/react-query";
import type { BrowseResponse } from "../../types/api";
import { apiFetch } from "./apiClient";

// ルート一覧 (マウントポイント) を取得
export function browseRootOptions() {
  return queryOptions({
    queryKey: ["browse"],
    queryFn: () => apiFetch<BrowseResponse>("/api/browse"),
    staleTime: 30 * 1000,
  });
}

// 特定ディレクトリの中身を取得
export function browseNodeOptions(nodeId: string | undefined) {
  return queryOptions({
    queryKey: ["browse", nodeId],
    queryFn: () => apiFetch<BrowseResponse>(`/api/browse/${nodeId}`),
    staleTime: 30 * 1000,
    enabled: !!nodeId,
  });
}
