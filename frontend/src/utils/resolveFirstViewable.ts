// ディレクトリ内の最初の閲覧対象を再帰的に探索する
// - selectFirstViewable の優先順位 (archive > pdf > image > directory) に従う
// - directory が選択された場合は再帰降下 (MAX_DEPTH まで)
// - ソート順を考慮し、ソート後の最初のエントリを選択する
// - 「▶ 開く」アクション・セット間ジャンプで使用

import type { QueryClient } from "@tanstack/react-query";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import type { SortOrder } from "../hooks/useViewerParams";
import { selectFirstViewable } from "../hooks/useFirstFile";
import type { BrowseEntry } from "../types/api";
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
  let currentNodeId = nodeId;

  for (let depth = 0; depth < MAX_DEPTH; depth++) {
    const data = await queryClient.fetchQuery(browseNodeOptions(currentNodeId));
    const sorted = sortEntries(data.entries, sort);
    const first = selectFirstViewable(sorted);
    if (!first) return null;

    // ディレクトリの場合はさらに中へ降下
    if (first.kind === "directory") {
      currentNodeId = first.node_id;
      continue;
    }

    // archive/pdf/image が見つかった
    return { entry: first, parentNodeId: data.current_node_id ?? currentNodeId };
  }

  return null;
}
