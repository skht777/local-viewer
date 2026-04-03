// ディレクトリ内の最初の閲覧対象を再帰的に探索する
// - selectFirstViewable の優先順位 (archive > pdf > image > directory) に従う
// - directory が選択された場合は再帰降下（MAX_DEPTH まで）
// - 「▶ 開く」アクションで使用し、「→ 進入」との差別化を実現する

import type { QueryClient } from "@tanstack/react-query";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import { selectFirstViewable } from "../hooks/useFirstFile";
import type { BrowseEntry } from "../types/api";

const MAX_DEPTH = 10;

export interface ResolvedTarget {
  entry: BrowseEntry;
  parentNodeId: string;
}

export async function resolveFirstViewable(
  nodeId: string,
  queryClient: QueryClient,
): Promise<ResolvedTarget | null> {
  let currentNodeId = nodeId;

  for (let depth = 0; depth < MAX_DEPTH; depth++) {
    const data = await queryClient.fetchQuery(browseNodeOptions(currentNodeId));
    const first = selectFirstViewable(data.entries);
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
