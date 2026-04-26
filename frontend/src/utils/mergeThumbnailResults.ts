// バッチサムネイル結果をマージする pure function
// - 各チャンク結果の thumbnails から data が存在する ID だけを Map に収集
// - Blob URL 変換や revoke は行わない（副作用は hook 側に残す）
//
// Finding 4 対応: useBatchThumbnails の chunkResults.map(r => r.dataUpdatedAt).join(",")
// を依存にした useMemo を、純粋な pure 関数 + 入力配列への直接依存に置き換える。

interface ThumbnailChunkResult {
  data?: {
    thumbnails: Record<string, { data?: string }>;
  };
}

/**
 * チャンク結果配列から node_id → base64 のマップを構築する。
 * エラー・undefined・data 欠損は無視する。
 */
export function mergeThumbnailQueryResults(
  chunkResults: ThumbnailChunkResult[],
): Map<string, string> {
  const merged = new Map<string, string>();
  for (const result of chunkResults) {
    if (result.data) {
      for (const [id, thumb] of Object.entries(result.data.thumbnails)) {
        if (thumb.data) {
          merged.set(id, thumb.data);
        }
      }
    }
  }
  return merged;
}
