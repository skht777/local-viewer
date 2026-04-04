// バッチサムネイル API フック
// - node_ids を収集して POST /api/thumbnails/batch に投げる
// - 結果を node_id → Blob URL のマップとしてキャッシュ
// - Blob URL は cleanup で revokeObjectURL する

import { useEffect, useMemo, useRef, useState } from "react";
import { apiPost } from "./apiClient";

interface ThumbnailResult {
  data?: string;
  etag?: string;
  error?: string;
  code?: string;
}

interface BatchResponse {
  thumbnails: Record<string, ThumbnailResult>;
}

// base64 → Blob URL 変換
function base64ToBlobUrl(base64: string): string {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  const blob = new Blob([bytes], { type: "image/jpeg" });
  return URL.createObjectURL(blob);
}

/**
 * バッチサムネイル取得フック
 *
 * node_ids が変わったらバッチリクエストを発行し、
 * node_id → Blob URL のマップを返す。
 * Blob URL は node_ids 変更時に自動 revoke される。
 */
export function useBatchThumbnails(nodeIds: string[]): Map<string, string> {
  const [urlMap, setUrlMap] = useState<Map<string, string>>(new Map());
  // 前回の Blob URLs を cleanup 用に保持
  const prevUrlsRef = useRef<string[]>([]);

  // node_ids の安定したキー (依存配列用)
  const key = useMemo(() => nodeIds.join(","), [nodeIds]);

  useEffect(() => {
    if (nodeIds.length === 0) {
      setUrlMap(new Map());
      return;
    }

    let cancelled = false;

    async function fetchBatch() {
      try {
        const resp = await apiPost<BatchResponse>("/api/thumbnails/batch", {
          node_ids: nodeIds,
        });

        if (cancelled) return;

        const newMap = new Map<string, string>();
        const newUrls: string[] = [];

        for (const [id, result] of Object.entries(resp.thumbnails)) {
          if (result.data) {
            const blobUrl = base64ToBlobUrl(result.data);
            newMap.set(id, blobUrl);
            newUrls.push(blobUrl);
          }
        }

        // 前回の Blob URL を解放
        for (const url of prevUrlsRef.current) {
          URL.revokeObjectURL(url);
        }
        prevUrlsRef.current = newUrls;

        setUrlMap(newMap);
      } catch {
        // エラー時はフォールバック (個別 <img src> が使われる)
      }
    }

    fetchBatch();

    return () => {
      cancelled = true;
    };
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps

  // アンマウント時に全 Blob URL を解放
  useEffect(() => {
    return () => {
      for (const url of prevUrlsRef.current) {
        URL.revokeObjectURL(url);
      }
    };
  }, []);

  return urlMap;
}
