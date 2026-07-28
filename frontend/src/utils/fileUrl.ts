// ファイル配信 API の URL を生成する
// - modifiedAt 指定時: ?v={整数} を付与し、ファイル差し替え時に HTTP キャッシュを自然に無効化する
//   (node_id はパス由来で内容が変わっても不変のため、URL 版数がないと max-age 内は stale が続く)
// - modifiedAt が null/undefined: バージョンなし（ETag フォールバック）
export function fileUrl(nodeId: string, modifiedAt?: number | null): string {
  const base = `/api/file/${nodeId}`;
  if (modifiedAt == null) {
    return base;
  }
  return `${base}?v=${Math.floor(modifiedAt)}`;
}
