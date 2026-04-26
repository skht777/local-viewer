// API レスポンスの型定義

export interface AncestorEntry {
  node_id: string;
  name: string;
}

export interface BrowseEntry {
  node_id: string;
  name: string;
  kind: "directory" | "image" | "video" | "pdf" | "archive" | "other";
  size_bytes: number | null;
  mime_type: string | null;
  child_count: number | null;
  modified_at: number | null;
  preview_node_ids: string[] | null;
}

export interface BrowseResponse {
  current_node_id: string | null;
  current_name: string;
  parent_node_id: string | null;
  ancestors: AncestorEntry[];
  entries: BrowseEntry[];
  next_cursor: string | null;
  total_count: number | null;
}

// First-viewable API
export interface FirstViewableResponse {
  entry: BrowseEntry | null;
  parent_node_id: string | null;
}

// Sibling API (単方向)
export interface SiblingResponse {
  entry: BrowseEntry | null;
}

// Siblings API (prev + next 一括)
export interface SiblingsResponse {
  prev: BrowseEntry | null;
  next: BrowseEntry | null;
}

// 検索 API
export interface SearchResult {
  node_id: string;
  parent_node_id: string | null;
  name: string;
  kind: BrowseEntry["kind"];
  relative_path: string;
  size_bytes: number | null;
  // 拡張フィールド（後方互換: バックエンドが値を返さない場合は undefined/null）
  modified_at?: number | null;
  mime_type?: string | null;
  child_count?: number | null;
  preview_node_ids?: string[] | null;
}

export interface SearchResponse {
  results: SearchResult[];
  has_more: boolean;
  query: string;
  is_stale?: boolean;
  next_offset?: number | null;
}
