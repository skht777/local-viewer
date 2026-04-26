// セット間ジャンプ用の再帰的兄弟探索フック
// - sibling API → fallback 全件取得は siblingNavigation.fetchSiblingOne に委譲
// - sourceTopDir は最初に取得できた parentData (level 0) で一度だけ算出
// - levelsUp / visited / MAX_DEPTH の停止条件はこのフック内に残置
// - useSetJump からの複数 callback で共有する (findSiblingRecursive を直接 useCallback)

import { useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { fetchSiblingOne, resolveInitialParent } from "../lib/siblingNavigation";
import { resolveTopLevelDir } from "./useSetNavigation";
import type { SortOrder } from "./useViewerParams";
import type { AncestorEntry, BrowseEntry, BrowseResponse } from "../types/api";

const MAX_DEPTH = 10;

export interface SetJumpSearchResult {
  target: BrowseEntry;
  levelsUp: number;
  searchDirData: BrowseResponse;
  sourceTopDir: string | null;
}

interface UseFindSiblingRecursiveProps {
  currentNodeId: string | null;
  parentNodeId: string | null;
  ancestors: AncestorEntry[];
  sort: SortOrder;
}

export type FindSiblingRecursive = (
  direction: "next" | "prev",
) => Promise<SetJumpSearchResult | null>;

export function useFindSiblingRecursive({
  currentNodeId,
  parentNodeId,
  ancestors,
  sort,
}: UseFindSiblingRecursiveProps): FindSiblingRecursive {
  const queryClient = useQueryClient();

  return useCallback(
    async (direction) => {
      if (!currentNodeId) {
        return null;
      }
      let currentChildId: string = currentNodeId;
      let currentParentId = resolveInitialParent(parentNodeId, ancestors);
      let levelsUp = 0;
      let sourceTopDir: string | null = null;
      let isSourceResolved = false;
      const visited = new Set<string>();

      while (currentParentId && levelsUp < MAX_DEPTH) {
        if (visited.has(currentParentId)) {
          break;
        }
        visited.add(currentParentId);

        const fetched = await fetchSiblingOne({
          queryClient,
          parentId: currentParentId,
          childId: currentChildId,
          sort,
          direction,
        });
        if (!fetched) {
          return null;
        }
        const { parentData, sibling } = fetched;

        // ソースの topDir を level 0 (最初に取得できた parentData) で算出
        if (!isSourceResolved) {
          const targetChildId = currentChildId;
          const sourceEntry = parentData.entries.find(
            (e: BrowseEntry) => e.node_id === targetChildId,
          );
          if (sourceEntry) {
            sourceTopDir = resolveTopLevelDir(
              parentData.ancestors,
              parentData.current_node_id,
              sourceEntry,
            );
          }
          isSourceResolved = true;
        }

        if (sibling) {
          return { target: sibling, levelsUp, searchDirData: parentData, sourceTopDir };
        }

        // 兄弟なし → 上に登る (parentData は再フェッチ不要)
        levelsUp++;
        if (!parentData.current_node_id) {
          return null;
        }
        currentChildId = parentData.current_node_id;
        currentParentId = resolveInitialParent(parentData.parent_node_id, parentData.ancestors);
      }

      return null;
    },
    [currentNodeId, parentNodeId, ancestors, sort, queryClient],
  );
}
