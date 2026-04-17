// チャンク別バッチサムネイルクエリ結果を dataUpdatedAt シグナルでマージする hook
// - useBatchThumbnails から lint 抑制依存を剥がすための API 境界
// - 依存配列の不変条件（dataUpdatedAt の変化でのみ再計算）を hook 名で表現する

import { useMemo } from "react";
import { mergeThumbnailQueryResults } from "../utils/mergeThumbnailResults";

interface ThumbnailChunkQueryResult {
  data?: {
    thumbnails: Record<string, { data?: string }>;
  };
  dataUpdatedAt: number;
}

/**
 * チャンク別クエリ結果を node_id → base64 のマップにマージする。
 *
 * 不変条件:
 * - 各 `chunkResults[i].dataUpdatedAt` が変化した場合にのみ再計算する。
 * - `chunkResults` 配列自体の参照変化（useQueries が毎レンダリング生成する新規配列）では
 *   再計算しない。`dataUpdatedAt` を連結した文字列キーで変化を検出する。
 * - 返り値 `Map` は dataUpdatedAt 不変の間は参照安定。
 *
 * この不変条件は React の exhaustive-deps では表現できないため、
 * 抑制コメントはこの hook 内部に閉じ込める。
 */
export function useMergedThumbnailData(
  chunkResults: ThumbnailChunkQueryResult[],
): Map<string, string> {
  const dataKey = chunkResults.map((r) => r.dataUpdatedAt).join(",");
  return useMemo(
    () => mergeThumbnailQueryResults(chunkResults),
    // dataKey は chunkResults の dataUpdatedAt を連結した代理値。
    // chunkResults 自体は毎レンダリング新規生成されるため依存に入れると useMemo が無意味化する。
    // biome-ignore lint/correctness/useExhaustiveDependencies: dataKey で変更を追跡する意図的設計
    [dataKey], // eslint-disable-line react-hooks/exhaustive-deps
  );
}
