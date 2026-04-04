// ソートキーと方向に応じてエントリを並び替え
// - name-asc: API のデフォルト順 (ディレクトリ優先 + 名前昇順) をそのまま使用
// - name-desc: 名前降順 (ディレクトリ優先は維持)
// - date-desc: 更新日時降順 (最新が先頭)、null は末尾
// - date-asc: 更新日時昇順 (最古が先頭)、null は末尾

import type { SortOrder } from "../hooks/useViewerParams";
import type { BrowseEntry } from "../types/api";

export function sortEntries(entries: BrowseEntry[], sort: SortOrder): BrowseEntry[] {
  if (sort === "name-asc") return entries;

  return [...entries].sort((a, b) => {
    if (sort === "name-desc") {
      // ディレクトリ優先は維持しつつ、名前は降順
      const aIsDir = a.kind === "directory" ? 0 : 1;
      const bIsDir = b.kind === "directory" ? 0 : 1;
      if (aIsDir !== bIsDir) return aIsDir - bIsDir;
      return b.name.localeCompare(a.name, undefined, { numeric: true, sensitivity: "base" });
    }

    // date ソート: null は末尾
    if (a.modified_at == null && b.modified_at == null) return 0;
    if (a.modified_at == null) return 1;
    if (b.modified_at == null) return -1;

    return sort === "date-desc" ? b.modified_at - a.modified_at : a.modified_at - b.modified_at;
  });
}
