// マウントポイント API の型定義

export interface MountEntry {
  mount_id: string;
  name: string;
  node_id: string;
  child_count: number | null;
}

export interface MountListResponse {
  mounts: MountEntry[];
}
