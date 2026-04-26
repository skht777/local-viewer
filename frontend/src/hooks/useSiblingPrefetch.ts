// 次/前セットのバックグラウンドプリフェッチ
// - ビューワー表示時に兄弟セットを事前探索してキャッシュを温める
// - /api/browse/{parent}/siblings で prev+next を一括取得（バックエンドスキャン1回）
// - 探索結果のセットの browse データと最初の数枚の画像もプリフェッチ
// - useSetJump の findSiblingRecursive がキャッシュヒットするようになり体感ラグ削減
// - PDF の兄弟は親ディレクトリを温めるのみ（/api/browse は PDF に 422 を返す）

import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { browseInfiniteOptions } from "./api/browseQueries";
import { fetchSiblingPair, resolveInitialParent, walkUpToParent } from "../lib/siblingNavigation";
import type { SortOrder } from "./useViewerParams";
import type { AncestorEntry, BrowseEntry } from "../types/api";

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

type QueryClient = ReturnType<typeof useQueryClient>;

interface SiblingPair {
  prev: BrowseEntry | null;
  next: BrowseEntry | null;
}

// ジャンプ先セットの browse データを infinite キャッシュに温め、最初の数枚の画像をプリロード
async function prefetchSiblingTarget(
  sibling: BrowseEntry,
  sort: SortOrder,
  queryClient: QueryClient,
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

// 見つかった方向を並列プリフェッチし、どの方向が完了したかを返す
async function prefetchFoundSiblings(
  queryClient: QueryClient,
  pair: SiblingPair,
  sort: SortOrder,
  cancelled: () => boolean,
): Promise<{ prevDone: boolean; nextDone: boolean }> {
  const tasks: Promise<void>[] = [];
  const result = { prevDone: false, nextDone: false };
  if (pair.prev) {
    result.prevDone = true;
    tasks.push(prefetchSiblingTarget(pair.prev, sort, queryClient, cancelled));
  }
  if (pair.next) {
    result.nextDone = true;
    tasks.push(prefetchSiblingTarget(pair.next, sort, queryClient, cancelled));
  }
  if (tasks.length > 0) {
    await Promise.all(tasks);
  }
  return result;
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
    const startNodeId = currentNodeId;

    // 同一コンテキストで既にプリフェッチ済みならスキップ
    const prefetchKey = `${startNodeId}:${parentNodeId}:${sort}`;
    if (prefetchedKeys.has(prefetchKey)) {
      return;
    }

    let cancelled = false;
    const isCancelled = () => cancelled;

    async function prefetchBothDirections() {
      let childId: string | null = startNodeId;
      let parentId = resolveInitialParent(parentNodeId, ancestors);
      let needPrev = true;
      let needNext = true;
      let levelsUp = 0;
      const visited = new Set<string>();

      while (parentId && childId && levelsUp < MAX_DEPTH && (needPrev || needNext)) {
        if (cancelled || visited.has(parentId)) {
          break;
        }
        visited.add(parentId);

        const pair = await fetchSiblingPair({
          queryClient,
          parentId,
          childId,
          sort,
          needPrev,
          needNext,
          cancelled: isCancelled,
        });
        if (!pair) {
          return;
        }
        const onlyMissing: SiblingPair = {
          prev: needPrev ? pair.prev : null,
          next: needNext ? pair.next : null,
        };
        const { prevDone, nextDone } = await prefetchFoundSiblings(
          queryClient,
          onlyMissing,
          sort,
          isCancelled,
        );
        if (prevDone) {
          needPrev = false;
        }
        if (nextDone) {
          needNext = false;
        }

        if (needPrev || needNext) {
          levelsUp++;
          const upper = await walkUpToParent({
            queryClient,
            currentParentId: parentId,
            sort,
          });
          if (!upper) {
            return;
          }
          ({ childId, parentId } = upper);
        }
      }
    }

    async function runPrefetch() {
      await prefetchBothDirections();
      if (!cancelled) {
        if (prefetchedKeys.size >= PREFETCHED_KEYS_MAX) {
          prefetchedKeys.clear();
        }
        prefetchedKeys.add(prefetchKey);
      }
    }
    runPrefetch();

    return () => {
      cancelled = true;
    };
  }, [currentNodeId, parentNodeId, ancestors, sort, queryClient]);
}
