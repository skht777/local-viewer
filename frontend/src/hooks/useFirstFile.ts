// ディレクトリ内の最初の表示対象を選択するユーティリティ
// 優先順位: archive > pdf > image > directory (再帰降下用)

import type { BrowseEntry } from "../types/api";

// 同期版: browse レスポンスのエントリから最初の表示対象を選ぶ
// archive/directory を返した場合は呼び出し側で再帰的に降下する
export function selectFirstViewable(entries: BrowseEntry[]): BrowseEntry | null {
  // 優先順位1: アーカイブ (中身を展開して閲覧)
  const firstArchive = entries.find((e) => e.kind === "archive");
  if (firstArchive) {
    return firstArchive;
  }

  // 優先順位2: PDF (ページ単位で閲覧)
  const firstPdf = entries.find((e) => e.kind === "pdf");
  if (firstPdf) {
    return firstPdf;
  }

  // 優先順位3: 画像
  const firstImage = entries.find((e) => e.kind === "image");
  if (firstImage) {
    return firstImage;
  }

  // 優先順位4: ディレクトリ (再帰降下の候補)
  const firstDir = entries.find((e) => e.kind === "directory");
  if (firstDir) {
    return firstDir;
  }

  return null;
}
