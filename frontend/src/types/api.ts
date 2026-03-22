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
