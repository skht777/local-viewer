// ディレクトリ内の最初の閲覧対象を再帰的に探索する
// - サーバー側 first-viewable API を優先使用 (DirIndex 対応)
// - API 失敗時はフォールバック: browseNodeOptions + クライアントサイド探索
// - 「▶ 開く」アクション・セット間ジャンプで使用

import type { QueryClient } from "@tanstack/react-query";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import { apiFetch } from "../hooks/api/apiClient";
import type { SortOrder } from "../hooks/useViewerParams";
import { selectFirstViewable } from "../hooks/useFirstFile";
import type { BrowseEntry, FirstViewableResponse } from "../types/api";
import { sortEntries } from "./sortEntries";

const MAX_DEPTH = 10;

export interface ResolvedTarget {
  entry: BrowseEntry;
  parentNodeId: string;
}

export async function resolveFirstViewable(
  nodeId: string,
  queryClient: QueryClient,
  sort: SortOrder = "name-asc",
): Promise<ResolvedTarget | null> {
  // サーバー側 first-viewable API を試行
  try {
    const resp = await apiFetch<FirstViewableResponse>(
      `/api/browse/${nodeId}/first-viewable?sort=${sort}`,
    );
    if (resp.entry && resp.parent_node_id) {
      return { entry: resp.entry, parentNodeId: resp.parent_node_id };
    }
    if (resp.entry === null) {
      return null;
    }
  } catch {
    // API 失敗時はフォールバック
  }

  // フォールバック: クライアントサイド再帰探索
  let currentNodeId = nodeId;

  for (let depth = 0; depth < MAX_DEPTH; depth++) {
    const data = await queryClient.fetchQuery(browseNodeOptions(currentNodeId, sort));
    const sorted = sortEntries(data.entries, sort);
    const first = selectFirstViewable(sorted);
    if (!first) {
      return null;
    }

    if (first.kind === "directory") {
      currentNodeId = first.node_id;
      continue;
    }

    return { entry: first, parentNodeId: data.current_node_id ?? currentNodeId };
  }

  return null;
}
