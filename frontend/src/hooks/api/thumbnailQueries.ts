// バッチサムネイル API フック
// - node_ids を収集して POST /api/thumbnails/batch に投げる
// - TanStack Query でキャッシュ・リトライ・dedup を活用
// - Query キャッシュには raw base64 data のみ、Blob URL はローカルで管理

import { useQuery } from "@tanstack/react-query";
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
  // デバウンス: 短時間の連続変更をまとめる (タブ切替・フィルタ変更対応)
  const [debouncedIds, setDebouncedIds] = useState(nodeIds);
  const key = useMemo(() => nodeIds.join(","), [nodeIds]);
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedIds(nodeIds), 50);
    return () => clearTimeout(timer);
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps

  const debouncedKey = useMemo(() => debouncedIds.join(","), [debouncedIds]);

  // TanStack Query: raw base64 data のみキャッシュ
  const { data: rawData } = useQuery({
    queryKey: ["thumbnails", "batch", debouncedKey],
    queryFn: async () => {
      const resp = await apiPost<BatchResponse>("/api/thumbnails/batch", {
        node_ids: debouncedIds,
      });
      // raw base64 data のみ抽出 (Blob URL はローカルで管理)
      const result = new Map<string, string>();
      for (const [id, thumb] of Object.entries(resp.thumbnails)) {
        if (thumb.data) {
          result.set(id, thumb.data);
        }
      }
      return result;
    },
    enabled: debouncedIds.length > 0,
    staleTime: 10 * 60 * 1000, // サムネイルは長時間キャッシュ可
  });

  // Blob URL のローカル管理 (Query キャッシュに載せない)
  const prevUrlsRef = useRef<string[]>([]);
  const urlMap = useMemo(() => {
    if (!rawData) return new Map<string, string>();

    // 前回の Blob URL を解放
    for (const url of prevUrlsRef.current) {
      URL.revokeObjectURL(url);
    }

    const newMap = new Map<string, string>();
    const newUrls: string[] = [];
    for (const [id, base64] of rawData) {
      const blobUrl = base64ToBlobUrl(base64);
      newMap.set(id, blobUrl);
      newUrls.push(blobUrl);
    }
    prevUrlsRef.current = newUrls;
    return newMap;
  }, [rawData]);

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
