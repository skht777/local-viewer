// 次/前セットのバックグラウンドプリフェッチ
// - ビューワー表示時に兄弟セットを事前探索してキャッシュを温める
// - /api/browse/{parent}/siblings で prev+next を一括取得（バックエンドスキャン1回）
// - 探索結果のセットの browse データと最初の数枚の画像もプリフェッチ
// - useSetJump の findSiblingRecursive がキャッシュヒットするようになり体感ラグ削減
// - PDF の兄弟は親ディレクトリを温めるのみ（/api/browse は PDF に 422 を返す）

import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { apiFetch } from "./api/apiClient";
import { browseInfiniteOptions, browseNodeOptions } from "./api/browseQueries";
import { findNextSet, findPrevSet } from "./useSetNavigation";
import type { SortOrder } from "./useViewerParams";
import type { AncestorEntry, BrowseEntry, BrowseResponse, SiblingsResponse } from "../types/api";

interface UseSiblingPrefetchProps {
  currentNodeId: string | null;
  parentNodeId: string | null;
  ancestors?: AncestorEntry[];
  sort?: SortOrder;
}

const MAX_DEPTH = 10;
const IMAGE_PRELOAD_COUNT = 3;
const PREFETCHED_KEYS_MAX = 500;

// ビューワー再マウントを跨いでもプリフェッチの重複実行を防止する
// currentNodeId + parentNodeId + sort をキーとして管理
export const prefetchedKeys = new Set<string>();

// ジャンプ先セットの browse データを infinite キャッシュに温め、最初の数枚の画像をプリロード
async function prefetchSiblingTarget(
  sibling: BrowseEntry,
  sort: SortOrder,
  queryClient: ReturnType<typeof useQueryClient>,
  cancelled: () => boolean,
): Promise<void> {
  if (sibling.kind !== "directory" && sibling.kind !== "archive") {
    return;
  }

  try {
    await queryClient.prefetchInfiniteQuery(browseInfiniteOptions(sibling.node_id, sort));
    if (cancelled()) {
      return;
    }
    const cached = queryClient.getQueryData(browseInfiniteOptions(sibling.node_id, sort).queryKey);
    const entries = cached?.pages?.[0]?.entries ?? [];
    const images = entries.filter((e) => e.kind === "image");
    const count = Math.min(IMAGE_PRELOAD_COUNT, images.length);
    for (let i = 0; i < count; i++) {
      const img = new Image();
      img.src = `/api/file/${images[i].node_id}`;
    }
  } catch {
    // ベストエフォート — エラー無視
  }
}

export function useSiblingPrefetch({
  currentNodeId,
  parentNodeId,
  ancestors = [],
  sort = "name-asc",
}: UseSiblingPrefetchProps): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!currentNodeId) {
      return;
    }

    // 同一コンテキストで既にプリフェッチ済みならスキップ
    const prefetchKey = `${currentNodeId}:${parentNodeId}:${sort}`;
    if (prefetchedKeys.has(prefetchKey)) {
      return;
    }

    let cancelled = false;
    const isCancelled = () => cancelled;

    async function prefetchBothDirections() {
      let currentChildId: string | null = currentNodeId;
      let currentParentId = parentNodeId;
      let levelsUp = 0;
      const visited = new Set<string>();

      // parentNodeId が null の場合、ancestors[0] (マウントルート) を使用
      if (!currentParentId) {
        if (ancestors.length === 0 || !currentNodeId) {
          return;
        }
        currentParentId = ancestors[0].node_id;
      }

      // prev/next の探索状態を個別に追跡
      let needPrev = true;
      let needNext = true;

      while (currentParentId && levelsUp < MAX_DEPTH && (needPrev || needNext)) {
        if (cancelled) {
          return;
        }
        if (visited.has(currentParentId)) {
          break;
        }
        visited.add(currentParentId);

        // combined siblings API で prev+next を一括取得
        let prevSibling: BrowseEntry | null = null;
        let nextSibling: BrowseEntry | null = null;

        if (currentChildId) {
          try {
            const resp = await apiFetch<SiblingsResponse>(
              `/api/browse/${currentParentId}/siblings?current=${currentChildId}&sort=${sort}`,
            );
            if (needPrev) {
              prevSibling = resp.prev;
            }
            if (needNext) {
              nextSibling = resp.next;
            }
          } catch {
            // フォールバック: 親ディレクトリの全件取得でクライアント側探索
          }
        }

        // API で見つからなかった方向はクライアント側フォールバック
        if ((needPrev && !prevSibling) || (needNext && !nextSibling)) {
          let parentData: BrowseResponse;
          try {
            parentData = await queryClient.fetchQuery(browseNodeOptions(currentParentId, sort));
          } catch {
            return;
          }
          if (!currentChildId || cancelled) {
            return;
          }

          if (needPrev && !prevSibling) {
            prevSibling = findPrevSet(parentData.entries, currentChildId);
          }
          if (needNext && !nextSibling) {
            nextSibling = findNextSet(parentData.entries, currentChildId);
          }
        }

        // 見つかった方向をプリフェッチし、探索完了とマーク
        const prefetches: Promise<void>[] = [];
        if (needPrev && prevSibling) {
          needPrev = false;
          prefetches.push(prefetchSiblingTarget(prevSibling, sort, queryClient, isCancelled));
        }
        if (needNext && nextSibling) {
          needNext = false;
          prefetches.push(prefetchSiblingTarget(nextSibling, sort, queryClient, isCancelled));
        }
        if (prefetches.length > 0) {
          await Promise.all(prefetches);
        }

        // まだ見つかっていない方向がある場合は上に登る
        if (needPrev || needNext) {
          levelsUp++;
          // 親ディレクトリのデータを取得して上へ
          let parentData: BrowseResponse;
          try {
            parentData = await queryClient.fetchQuery(browseNodeOptions(currentParentId, sort));
          } catch {
            return;
          }
          currentChildId = parentData.current_node_id;
          currentParentId = parentData.parent_node_id;

          if (!currentParentId && parentData.ancestors.length > 0) {
            currentParentId = parentData.ancestors[0].node_id;
          }
        }
      }
    }

    prefetchBothDirections().then(() => {
      if (!cancelled) {
        if (prefetchedKeys.size >= PREFETCHED_KEYS_MAX) {
          prefetchedKeys.clear();
        }
        prefetchedKeys.add(prefetchKey);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [currentNodeId, parentNodeId, ancestors, sort, queryClient]);
}
