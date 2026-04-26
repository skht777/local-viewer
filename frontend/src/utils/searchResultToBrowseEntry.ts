// 検索結果 (SearchResult) を BrowseEntry に変換するアダプタ
//
// FileBrowser/FileCard が BrowseEntry 型に依存するため、検索結果ページでも
// 同じコンポーネントを再利用できるようにここで型を揃える。
// バックエンドから返らない拡張フィールド（modified_at 等）は null フォールバック。

import type { BrowseEntry, SearchResult } from "../types/api";

export function searchResultToBrowseEntry(result: SearchResult): BrowseEntry {
  return {
    child_count: result.child_count ?? null,
    kind: result.kind === "other" ? "other" : result.kind,
    mime_type: result.mime_type ?? null,
    modified_at: result.modified_at ?? null,
    name: result.name,
    node_id: result.node_id,
    preview_node_ids: result.preview_node_ids ?? null,
    size_bytes: result.size_bytes,
  };
}
