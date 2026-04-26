// セット間ジャンプのツリー走査ロジック
// - ディレクトリ + アーカイブ + PDF がセット候補
// - 同ディレクトリ内の次/前のセット候補を探す純粋関数
// - 再帰的なツリー走査はフック側で API を呼んで実行

import type { AncestorEntry, BrowseEntry } from "../types/api";

// セット候補かどうかを判定する (PDF は Phase 6 で追加)
function isSetCandidate(e: BrowseEntry): boolean {
  return e.kind === "directory" || e.kind === "archive" || e.kind === "pdf";
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

  if (currentIdx <= 0) {
    return null;
  }

  return candidates[currentIdx - 1] ?? null;
}

// 探索ディレクトリの ancestors と対象エントリからトップレベルディレクトリを算出
// - ancestors = [mount_root, topLevelDir, ...] → ancestors[1]
// - ancestors = [mount_root] → 探索ディレクトリ自体が topDir
// - ancestors = [] → マウントルート直下 → ディレクトリなら自身が topDir、ファイルなら null
export function resolveTopLevelDir(
  ancestors: AncestorEntry[],
  searchDirNodeId: string | null,
  entry: BrowseEntry,
): string | null {
  if (ancestors.length >= 2) {
    return ancestors[1].node_id;
  }
  if (ancestors.length === 1) {
    return searchDirNodeId;
  }
  return entry.kind === "directory" ? entry.node_id : null;
}

// セット間ジャンプで確認ダイアログを出すべきか判定
// - 条件 B: 2階層以上上がって兄弟を探した場合
// - 条件 A: トップレベルDir が変わる場合（null ↔ non-null も含む）
export function shouldConfirm(
  levelsUp: number,
  sourceTopDir: string | null,
  targetTopDir: string | null,
): boolean {
  if (levelsUp >= 2) {
    return true;
  }
  if (sourceTopDir !== targetTopDir) {
    return true;
  }
  return false;
}
