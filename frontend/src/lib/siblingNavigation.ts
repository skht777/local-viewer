// セット間ジャンプ / プリフェッチで共有する兄弟ノード探索ロジック
// - useSetJump (単方向探索 + navigate) と useSiblingPrefetch (双方向プリフェッチ) の
//   API 差異を吸収する 4 関数を提供する
// - 探索停止条件 (visited / MAX_DEPTH / sourceTopDir 不変条件) は呼び出し側に残す

import type { QueryClient } from "@tanstack/react-query";
import { apiFetch } from "../hooks/api/apiClient";
import { browseNodeOptions } from "../hooks/api/browseQueries";
import { findNextSet, findPrevSet } from "../hooks/useSetNavigation";
import { sortEntries } from "../utils/sortEntries";
import type { SortOrder } from "../hooks/useViewerParams";
import type {
  AncestorEntry,
  BrowseEntry,
  BrowseResponse,
  SiblingResponse,
  SiblingsResponse,
} from "../types/api";

// 兄弟探索の起点となる親 nodeId を解決する純粋関数
// - parentNodeId が null かつ ancestors[0] が存在すればマウントルートを採用
// - どちらも欠けていれば null (探索不能)
export function resolveInitialParent(
  parentNodeId: string | null,
  ancestors: AncestorEntry[],
): string | null {
  if (parentNodeId) {
    return parentNodeId;
  }
  if (ancestors.length === 0) {
    return null;
  }
  return ancestors[0].node_id;
}

interface FetchSiblingPairParams {
  queryClient: QueryClient;
  parentId: string;
  childId: string;
  sort: SortOrder;
  needPrev: boolean;
  needNext: boolean;
  cancelled?: () => boolean;
}

interface SiblingPairResult {
  parentData: BrowseResponse | null;
  prev: BrowseEntry | null;
  next: BrowseEntry | null;
}

// 一括取得版: useSiblingPrefetch 用
// - /api/browse/{parent}/siblings で prev+next を一度に取得
// - needPrev/needNext で必要側だけ抽出
// - 欠けた方向はクライアント側で sortEntries + findNext/Prev で fallback
// - 親ディレクトリ取得失敗 / cancelled / parentData 不在は null (=探索中断)
export async function fetchSiblingPair({
  queryClient,
  parentId,
  childId,
  sort,
  needPrev,
  needNext,
  cancelled,
}: FetchSiblingPairParams): Promise<SiblingPairResult | null> {
  let prev: BrowseEntry | null = null;
  let next: BrowseEntry | null = null;
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

  let parentData: BrowseResponse | null = null;
  if ((needPrev && !prev) || (needNext && !next)) {
    try {
      parentData = await queryClient.fetchQuery(browseNodeOptions(parentId, sort));
    } catch {
      return null;
    }
    if (!parentData || cancelled?.()) {
      return null;
    }
    const sorted = sortEntries(parentData.entries, sort);
    if (needPrev && !prev) {
      prev = findPrevSet(sorted, childId);
    }
    if (needNext && !next) {
      next = findNextSet(sorted, childId);
    }
  }

  return { parentData, prev, next };
}

interface FetchSiblingOneParams {
  queryClient: QueryClient;
  parentId: string;
  childId: string;
  sort: SortOrder;
  direction: "next" | "prev";
}

interface SiblingOneResult {
  parentData: BrowseResponse;
  sibling: BrowseEntry | null;
}

// 単方向取得版: useSetJump 用
// - /api/browse/{parent}/sibling で次/前を 1 クエリで試行
// - 結果が無くても fallback で parentData を取得し sortEntries → findNext/Prev で再検索
// - 戻り値の parentData は呼び出し側 (useSetJump) で sourceTopDir を一度だけ算出するため
export async function fetchSiblingOne({
  queryClient,
  parentId,
  childId,
  sort,
  direction,
}: FetchSiblingOneParams): Promise<SiblingOneResult | null> {
  let sibling: BrowseEntry | null = null;
  try {
    const resp = await apiFetch<SiblingResponse>(
      `/api/browse/${parentId}/sibling?current=${childId}&direction=${direction}&sort=${sort}`,
    );
    sibling = resp.entry;
  } catch {
    // API 失敗時はフォールバック
  }

  let parentData: BrowseResponse | null = null;
  try {
    parentData = await queryClient.fetchQuery(browseNodeOptions(parentId, sort));
  } catch {
    return null;
  }
  if (!parentData) {
    return null;
  }

  if (!sibling) {
    const sorted = sortEntries(parentData.entries, sort);
    const finder = direction === "next" ? findNextSet : findPrevSet;
    sibling = finder(sorted, childId);
  }
  return { parentData, sibling };
}

interface WalkUpToParentParams {
  queryClient: QueryClient;
  currentParentId: string;
  sort: SortOrder;
}

interface WalkUpResult {
  childId: string;
  parentId: string | null;
  parentData: BrowseResponse;
}

// 1 階層上に登るための { childId, parentId, parentData } を返す
// - 現在 parentId のノードを新たな child、その親を新たな parent とする
// - parent_node_id が null ならマウントルート (ancestors[0]) にフォールバック
// - 取得失敗 / parentData 不在 / current_node_id 欠落は null (=中断)
export async function walkUpToParent({
  queryClient,
  currentParentId,
  sort,
}: WalkUpToParentParams): Promise<WalkUpResult | null> {
  let parentData: BrowseResponse | null = null;
  try {
    parentData = await queryClient.fetchQuery(browseNodeOptions(currentParentId, sort));
  } catch {
    return null;
  }
  if (!parentData?.current_node_id) {
    return null;
  }
  let parentId = parentData.parent_node_id;
  if (!parentId && parentData.ancestors.length > 0) {
    parentId = parentData.ancestors[0].node_id;
  }
  return { childId: parentData.current_node_id, parentId, parentData };
}
