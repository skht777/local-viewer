// サムネイル API の URL を生成する
export function thumbnailUrl(nodeId: string): string {
  return `/api/thumbnail/${nodeId}`;
}
