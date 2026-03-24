// セット間ジャンプのツリー走査ロジック
// - Phase 2: ディレクトリのみがセット候補（archive/PDF はスキップ）
// - 同ディレクトリ内の次/前のサブディレクトリを探す純粋関数
// - 再帰的なツリー走査はフック側で API を呼んで実行

import type { BrowseEntry } from "../types/api";

// 同階層のエントリから、currentNodeId の次のセット候補を探す
// Phase 2 では directory のみ対象
export function findNextSet(siblings: BrowseEntry[], currentNodeId: string): BrowseEntry | null {
  const directories = siblings.filter((e) => e.kind === "directory");
  const currentIdx = directories.findIndex((e) => e.node_id === currentNodeId);

  // 現在の nodeId が見つからない場合は最初のディレクトリ
  if (currentIdx === -1) {
    return directories[0] ?? null;
  }

  // 次のディレクトリ
  return directories[currentIdx + 1] ?? null;
}

// 同階層のエントリから、currentNodeId の前のセット候補を探す
export function findPrevSet(siblings: BrowseEntry[], currentNodeId: string): BrowseEntry | null {
  const directories = siblings.filter((e) => e.kind === "directory");
  const currentIdx = directories.findIndex((e) => e.node_id === currentNodeId);

  if (currentIdx <= 0) return null;

  return directories[currentIdx - 1] ?? null;
}
