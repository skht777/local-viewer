// サムネイル API の URL を生成する
// - modifiedAt 指定時: ?v={整数} を付与し immutable キャッシュを有効化
// - modifiedAt が null/undefined: バージョンなし（ETag フォールバック）
export function thumbnailUrl(nodeId: string, modifiedAt?: number | null): string {
  const base = `/api/thumbnail/${nodeId}`;
  if (modifiedAt == null) return base;
  return `${base}?v=${Math.floor(modifiedAt)}`;
}
