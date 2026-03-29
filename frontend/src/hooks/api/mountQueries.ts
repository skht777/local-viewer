// マウントポイント API の TanStack Query オプション定義

import { queryOptions } from "@tanstack/react-query";
import type { MountListResponse } from "../../types/mount";
import { apiFetch } from "./apiClient";

// マウントポイント一覧を取得 (TopPage 用)
export function mountListOptions() {
  return queryOptions({
    queryKey: ["mounts"],
    queryFn: () => apiFetch<MountListResponse>("/api/mounts"),
  });
}
