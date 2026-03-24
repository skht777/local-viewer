// ディレクトリ内の最初の表示対象を選択するユーティリティ
// Phase 2 の優先順位: image > directory（再帰降下用）
// archive/PDF は Phase 2 ではスキップ

import type { BrowseEntry } from "../types/api";

// 同期版: browse レスポンスのエントリから最初の表示対象を選ぶ
// directory を返した場合は呼び出し側で再帰的に降下する
export function selectFirstViewable(entries: BrowseEntry[]): BrowseEntry | null {
  // 優先順位1: 画像
  const firstImage = entries.find((e) => e.kind === "image");
  if (firstImage) return firstImage;

  // 優先順位2: ディレクトリ（再帰降下の候補）
  const firstDir = entries.find((e) => e.kind === "directory");
  if (firstDir) return firstDir;

  // archive/PDF/video/other はスキップ
  return null;
}
