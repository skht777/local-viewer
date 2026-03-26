// API レスポンスの型定義

export interface BrowseEntry {
  node_id: string;
  name: string;
  kind: "directory" | "image" | "video" | "pdf" | "archive" | "other";
  size_bytes: number | null;
  mime_type: string | null;
  child_count: number | null;
}

export interface BrowseResponse {
  current_node_id: string | null;
  current_name: string;
  parent_node_id: string | null;
  entries: BrowseEntry[];
}

// 検索 API
export interface SearchResult {
  node_id: string;
  parent_node_id: string | null;
  name: string;
  kind: BrowseEntry["kind"];
  relative_path: string;
  size_bytes: number | null;
}

export interface SearchResponse {
  results: SearchResult[];
  has_more: boolean;
  query: string;
}
