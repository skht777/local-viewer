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

type QueryClient = ReturnType<typeof useQueryClient>;

interface ParentRef {
  childId: string;
  parentId: string;
}

interface SiblingPair {
  prev: BrowseEntry | null;
  next: BrowseEntry | null;
}

// 探索の起点 (childId, parentId) を決定する
// - parentNodeId が null かつ ancestors[0] が存在すればマウントルートを親に採用
// - currentNodeId 不在 / 親解決不能なら null
function resolveInitialParent(
  currentNodeId: string,
  parentNodeId: string | null,
  ancestors: AncestorEntry[],
): ParentRef | null {
  if (parentNodeId) {
    return { childId: currentNodeId, parentId: parentNodeId };
  }
  if (ancestors.length === 0) {
    return null;
  }
  return { childId: currentNodeId, parentId: ancestors[0].node_id };
}

interface FetchMissingSiblingsParams {
  queryClient: QueryClient;
  parentId: string;
  childId: string;
  sort: SortOrder;
  needPrev: boolean;
  needNext: boolean;
  cancelled: () => boolean;
}

// 不足している方向の兄弟を取得する
// - まず /api/browse/{parent}/siblings でまとめて取り、欠けたら親ディレクトリ全件で fallback
// - 親ディレクトリ取得失敗 / cancel / parentData 不在 は null（=探索中断）
async function fetchMissingSiblings({
  queryClient,
  parentId,
  childId,
  sort,
  needPrev,
  needNext,
  cancelled,
}: FetchMissingSiblingsParams): Promise<SiblingPair | null> {
  let prev: BrowseEntry | null = null;
  let next: BrowseEntry | null = null;

  // combined siblings API で prev+next を一括取得
  try {
    const resp = await apiFetch<SiblingsResponse>(
      `/api/browse/${parentId}/siblings?current=${childId}&sort=${sort}`,
    );
    if (needPrev) {
      ({ prev } = resp);
    }
    if (needNext) {
      ({ next } = resp);
    }
  } catch {
    // フォールバック: 親ディレクトリの全件取得でクライアント側探索
  }

  // API で見つからなかった方向はクライアント側フォールバック
  if ((needPrev && !prev) || (needNext && !next)) {
    let parentData: BrowseResponse | undefined = undefined;
    try {
      parentData = await queryClient.fetchQuery(browseNodeOptions(parentId, sort));
    } catch {
      return null;
    }
    if (!parentData || cancelled()) {
      return null;
    }
    if (needPrev && !prev) {
      prev = findPrevSet(parentData.entries, childId);
    }
    if (needNext && !next) {
      next = findNextSet(parentData.entries, childId);
    }
  }

  return { prev, next };
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

// 探索を 1 階層上に登る
// - 現在 parentId のノード自体を新たな child とし、その親を新たな parent とする
// - parent_node_id が null ならマウントルート（ancestors[0]）にフォールバック
// - 取得失敗 / parentData 不在は null（=中断）
async function walkUpToParent(
  queryClient: QueryClient,
  parentId: string,
  sort: SortOrder,
): Promise<{ childId: string | null; parentId: string | null } | null> {
  let parentData: BrowseResponse | undefined = undefined;
  try {
    parentData = await queryClient.fetchQuery(browseNodeOptions(parentId, sort));
  } catch {
    return null;
  }
  if (!parentData) {
    return null;
  }
  const { current_node_id: newChildId } = parentData;
  let newParentId = parentData.parent_node_id;
  if (!newParentId && parentData.ancestors.length > 0) {
    newParentId = parentData.ancestors[0].node_id;
  }
  return { childId: newChildId, parentId: newParentId };
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
      const initial = resolveInitialParent(startNodeId, parentNodeId, ancestors);
      if (!initial) {
        return;
      }
      let { childId, parentId }: { childId: string | null; parentId: string | null } = initial;
      let needPrev = true;
      let needNext = true;
      let levelsUp = 0;
      const visited = new Set<string>();

      while (parentId && childId && levelsUp < MAX_DEPTH && (needPrev || needNext)) {
        if (cancelled || visited.has(parentId)) {
          break;
        }
        visited.add(parentId);

        const pair = await fetchMissingSiblings({
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
          const next = await walkUpToParent(queryClient, parentId, sort);
          if (!next) {
            return;
          }
          ({ childId, parentId } = next);
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
