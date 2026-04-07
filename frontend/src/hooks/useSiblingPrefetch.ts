// 次/前セットのバックグラウンドプリフェッチ
// - ビューワー表示時に兄弟セットを事前探索してキャッシュを温める
// - 探索結果のセットの browse データと最初の数枚の画像もプリフェッチ
// - useSetJump の findSiblingRecursive がキャッシュヒットするようになり体感ラグ削減
// - PDF の兄弟は親ディレクトリを温めるのみ（/api/browse は PDF に 422 を返す）

import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { apiFetch } from "./api/apiClient";
import { browseNodeOptions } from "./api/browseQueries";
import { findNextSet, findPrevSet } from "./useSetNavigation";
import type { SortOrder } from "./useViewerParams";
import type { AncestorEntry, BrowseResponse, SiblingResponse } from "../types/api";

interface UseSiblingPrefetchProps {
  currentNodeId: string | null;
  parentNodeId: string | null;
  ancestors?: AncestorEntry[];
  sort?: SortOrder;
}

const MAX_DEPTH = 10;
const IMAGE_PRELOAD_COUNT = 3;

export function useSiblingPrefetch({
  currentNodeId,
  parentNodeId,
  ancestors = [],
  sort = "name-asc",
}: UseSiblingPrefetchProps): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!currentNodeId) return;

    let cancelled = false;

    // 指定方向の兄弟セットを探索し、browse データ + 画像をプリフェッチ
    async function prefetchDirection(direction: "next" | "prev") {
      let currentChildId: string | null = currentNodeId;
      let currentParentId = parentNodeId;
      let levelsUp = 0;
      const visited = new Set<string>();

      // parentNodeId が null の場合、ancestors[0] (マウントルート) を使用
      if (!currentParentId) {
        if (ancestors.length === 0 || !currentNodeId) return;
        currentParentId = ancestors[0].node_id;
      }

      const finder = direction === "next" ? findNextSet : findPrevSet;

      while (currentParentId && levelsUp < MAX_DEPTH) {
        if (cancelled) return;
        if (visited.has(currentParentId)) break;
        visited.add(currentParentId);

        // sibling API を優先試行
        let sibling = null;
        if (currentChildId) {
          try {
            const resp = await apiFetch<SiblingResponse>(
              `/api/browse/${currentParentId}/sibling?current=${currentChildId}&direction=${direction}&sort=${sort}`,
            );
            sibling = resp.entry;
          } catch {
            // フォールバック
          }
        }

        let parentData: BrowseResponse;
        try {
          parentData = await queryClient.fetchQuery(browseNodeOptions(currentParentId, sort));
        } catch {
          return;
        }
        if (!currentChildId || cancelled) return;

        if (!sibling) {
          sibling = finder(parentData.entries, currentChildId);
        }

        if (sibling) {
          // 兄弟発見 → kind に応じてプリフェッチ
          if (sibling.kind === "directory" || sibling.kind === "archive") {
            // ジャンプ先の browse データを取得 + 画像プリロード
            try {
              const targetData = await queryClient.fetchQuery(browseNodeOptions(sibling.node_id));
              if (cancelled) return;
              const images = targetData.entries.filter((e) => e.kind === "image");
              const count = Math.min(IMAGE_PRELOAD_COUNT, images.length);
              for (let i = 0; i < count; i++) {
                const img = new Image();
                img.src = `/api/file/${images[i].node_id}`;
              }
            } catch {
              // ベストエフォート — エラー無視
            }
          }
          // PDF の場合: 親ディレクトリは走査で既にキャッシュ済み → 追加処理なし
          return;
        }

        // 兄弟なし → 上に登る
        levelsUp++;
        currentChildId = parentData.current_node_id;
        currentParentId = parentData.parent_node_id;

        // parent_node_id が null → ancestors から mount root を取得
        if (!currentParentId && parentData.ancestors.length > 0) {
          currentParentId = parentData.ancestors[0].node_id;
        }
      }
    }

    // 両方向を並列実行
    prefetchDirection("next");
    prefetchDirection("prev");

    return () => {
      cancelled = true;
    };
  }, [currentNodeId, parentNodeId, ancestors, sort, queryClient]);
}
