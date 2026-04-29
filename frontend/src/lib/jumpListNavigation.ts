// 任意リスト範囲のセット間ジャンプ
// - viewer の現在 nodeId をリスト内で隣接エントリに進める純粋関数
// - リスト未登録 / 境界 (先頭で prev、末尾で next) / 空リストは null を返す
// - FS sibling フォールバックは行わない (呼び出し側で onBoundary を出す)

import type { BrowseEntry, SearchResult } from "../types/api";

export interface JumpListEntry {
  node_id: string;
  parent_node_id: string | null;
  kind: BrowseEntry["kind"];
  name: string;
}

// useSetJump.navigateToTarget の引数型 (BrowseEntry 全体への結合を切る)
export type SetJumpTarget = Pick<BrowseEntry, "node_id" | "kind" | "name">;

// 任意のジャンプリスト内で次/前のエントリを返す
// - currentNodeId が null またはリスト外: 境界扱い (null)
// - 末尾で next / 先頭で prev: 境界扱い (null、ループしない)
export function findInJumpList(
  direction: "next" | "prev",
  currentNodeId: string | null,
  list: JumpListEntry[],
): JumpListEntry | null {
  if (!currentNodeId || list.length === 0) {
    return null;
  }
  const idx = list.findIndex((e) => e.node_id === currentNodeId);
  if (idx === -1) {
    return null;
  }
  const nextIdx = direction === "next" ? idx + 1 : idx - 1;
  if (nextIdx < 0 || nextIdx >= list.length) {
    return null;
  }
  return list[nextIdx];
}

// 検索結果から jumpList を構築する
// - directory / archive / pdf のみを残す (image / video / other は除外)
// - 入力順序を保つ (検索結果の現在 sort 順を維持)
// - parent_node_id は SearchResult から transfer (PDF の親 dir 解決に必須)
export function buildJumpListFromSearch(entries: SearchResult[]): JumpListEntry[] {
  return entries
    .filter((e) => e.kind === "directory" || e.kind === "archive" || e.kind === "pdf")
    .map((e) => ({
      node_id: e.node_id,
      parent_node_id: e.parent_node_id,
      kind: e.kind,
      name: e.name,
    }));
}

// useSetJump の jumpList 経路を純粋関数化したアクション判定
// - "fallback": jumpList が null → 既存 FS sibling 経路へ
// - "boundary": リスト境界 / 未登録 ID / PDF parent null → onBoundary でメッセージ表示
// - "navigate": navigateToTarget で遷移
export type JumpListAction =
  | { type: "fallback" }
  | { type: "boundary"; message: string }
  | { type: "navigate"; target: JumpListEntry };

export function resolveJumpListAction(
  direction: "next" | "prev",
  currentNodeId: string | null,
  list: JumpListEntry[] | null,
): JumpListAction {
  if (!list) {
    return { type: "fallback" };
  }
  const target = findInJumpList(direction, currentNodeId, list);
  if (!target) {
    return {
      type: "boundary",
      message: direction === "next" ? "最後のセットです" : "最初のセットです",
    };
  }
  if (target.kind === "pdf" && target.parent_node_id === null) {
    return { type: "boundary", message: "セットを開けません" };
  }
  return { type: "navigate", target };
}
