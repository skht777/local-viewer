// セット間ジャンプのツリー走査ロジック
// - ディレクトリ + アーカイブがセット候補 (PDF は Phase 6 で追加)
// - 同ディレクトリ内の次/前のセット候補を探す純粋関数
// - 再帰的なツリー走査はフック側で API を呼んで実行

import type { BrowseEntry } from "../types/api";

// セット候補かどうかを判定する
function isSetCandidate(e: BrowseEntry): boolean {
  return e.kind === "directory" || e.kind === "archive";
}

// 同階層のエントリから、currentNodeId の次のセット候補を探す
export function findNextSet(siblings: BrowseEntry[], currentNodeId: string): BrowseEntry | null {
  const candidates = siblings.filter(isSetCandidate);
  const currentIdx = candidates.findIndex((e) => e.node_id === currentNodeId);

  // 現在の nodeId が見つからない場合は最初の候補
  if (currentIdx === -1) {
    return candidates[0] ?? null;
  }

  // 次の候補
  return candidates[currentIdx + 1] ?? null;
}

// 同階層のエントリから、currentNodeId の前のセット候補を探す
export function findPrevSet(siblings: BrowseEntry[], currentNodeId: string): BrowseEntry | null {
  const candidates = siblings.filter(isSetCandidate);
  const currentIdx = candidates.findIndex((e) => e.node_id === currentNodeId);

  if (currentIdx <= 0) return null;

  return candidates[currentIdx - 1] ?? null;
}
