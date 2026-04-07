// ソートキーと方向に応じてエントリを並び替え
// - name-asc/desc: ディレクトリ優先 + localeCompare (numeric) で統一
// - date-desc: 更新日時降順 (最新が先頭)、null は末尾
// - date-asc: 更新日時昇順 (最古が先頭)、null は末尾

import type { SortOrder } from "../hooks/useViewerParams";
import type { BrowseEntry } from "../types/api";

// ディレクトリ優先 + 名前の自然順比較
function compareByName(a: BrowseEntry, b: BrowseEntry): number {
  const aIsDir = a.kind === "directory" ? 0 : 1;
  const bIsDir = b.kind === "directory" ? 0 : 1;
  if (aIsDir !== bIsDir) return aIsDir - bIsDir;
  return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
}

export function sortEntries(entries: BrowseEntry[], sort: SortOrder): BrowseEntry[] {
  return [...entries].sort((a, b) => {
    if (sort === "name-asc") return compareByName(a, b);
    if (sort === "name-desc") return -compareByName(a, b);

    // date ソート: null は末尾、同一日時は名前昇順タイブレーカー (Windows Explorer 準拠)
    if (a.modified_at == null && b.modified_at == null) return 0;
    if (a.modified_at == null) return 1;
    if (b.modified_at == null) return -1;

    const dateCmp =
      sort === "date-desc" ? b.modified_at - a.modified_at : a.modified_at - b.modified_at;
    if (dateCmp !== 0) return dateCmp;
    return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
  });
}
