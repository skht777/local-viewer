// バッチサムネイル API フック
// - node_ids を 50 件チャンクに分割して POST /api/thumbnails/batch を並列リクエスト
// - TanStack Query の useQueries でチャンク別キャッシュ・リトライ・dedup を活用
// - Query キャッシュには raw base64 data のみ、Blob URL はローカルで差分管理

import { useQueries } from "@tanstack/react-query";
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

const BATCH_SIZE = 50;

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

// 配列を指定サイズのチャンクに分割
function splitIntoChunks<T>(arr: T[], size: number): T[][] {
  const chunks: T[][] = [];
  for (let i = 0; i < arr.length; i += size) {
    chunks.push(arr.slice(i, i + size));
  }
  return chunks;
}

// 優先 ID を先頭に並べ替え (元配列の相対順序は保持)
function prioritize(ids: string[], priorityIds?: Set<string>): string[] {
  if (!priorityIds || priorityIds.size === 0) return ids;
  const priority: string[] = [];
  const rest: string[] = [];
  for (const id of ids) {
    if (priorityIds.has(id)) {
      priority.push(id);
    } else {
      rest.push(id);
    }
  }
  return [...priority, ...rest];
}

/**
 * バッチサムネイル取得フック
 *
 * node_ids を 50 件チャンクに分割して並列バッチリクエストを発行し、
 * node_id → Blob URL のマップを返す。
 * priorityIds を渡すとビューポート内 ID が先頭チャンクに配置される。
 * Blob URL は差分管理: 共通 ID は再利用、不要分のみ revoke。
 */
export function useBatchThumbnails(
  nodeIds: string[],
  priorityIds?: Set<string>,
): { thumbnails: Map<string, string>; isLoading: boolean } {
  // デバウンス: 短時間の連続変更をまとめる (タブ切替・フィルタ変更対応)
  const [debouncedIds, setDebouncedIds] = useState(nodeIds);
  const key = useMemo(() => nodeIds.join(","), [nodeIds]);
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedIds(nodeIds), 50);
    return () => clearTimeout(timer);
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps

  // 優先 ID を先頭に並べ替え、50 件チャンクに分割
  const chunks = useMemo(
    () => splitIntoChunks(prioritize(debouncedIds, priorityIds), BATCH_SIZE),
    [debouncedIds, priorityIds],
  );

  // useQueries: チャンク別に並列バッチリクエスト
  // queryKey にはソート済み ID を使用 → タブ切替でチャンク境界が変わってもキャッシュヒット
  const chunkResults = useQueries({
    queries: chunks.map((chunk) => ({
      queryKey: ["thumbnails", "batch", [...chunk].sort().join(",")],
      queryFn: async () => {
        const resp = await apiPost<BatchResponse>("/api/thumbnails/batch", {
          node_ids: chunk,
        });
        return resp;
      },
      enabled: chunk.length > 0,
      staleTime: 10 * 60 * 1000,
    })),
  });

  // 全チャンク結果をマージ
  const chunkDataList = chunkResults.map((r) => r.data);
  const rawData = useMemo(() => {
    const merged = new Map<string, string>();
    for (const data of chunkDataList) {
      if (data) {
        for (const [id, thumb] of Object.entries(data.thumbnails)) {
          if (thumb.data) {
            merged.set(id, thumb.data);
          }
        }
      }
    }
    return merged;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chunkDataList.map((d) => d).join(",")]);

  // Blob URL の差分管理 (共通 ID は再利用、不要分のみ revoke)
  const prevUrlsRef = useRef(new Map<string, string>());
  const urlMap = useMemo(() => {
    if (rawData.size === 0) return new Map<string, string>();

    const newMap = new Map<string, string>();
    // rawData にある ID: 既存 URL 再利用 or 新規作成
    for (const [id, base64] of rawData) {
      const existing = prevUrlsRef.current.get(id);
      if (existing) {
        newMap.set(id, existing);
      } else {
        newMap.set(id, base64ToBlobUrl(base64));
      }
    }
    // 不要な URL のみ revoke
    for (const [id, url] of prevUrlsRef.current) {
      if (!newMap.has(id)) {
        URL.revokeObjectURL(url);
      }
    }
    prevUrlsRef.current = newMap;
    return newMap;
  }, [rawData]);

  // アンマウント時に全 Blob URL を解放
  useEffect(() => {
    return () => {
      for (const url of prevUrlsRef.current.values()) {
        URL.revokeObjectURL(url);
      }
    };
  }, []);

  // ローディング状態: デバウンス待ちまたはチャンク取得中
  const debouncedKey = useMemo(() => debouncedIds.join(","), [debouncedIds]);
  const isDebouncing = key !== debouncedKey;
  const isLoading = nodeIds.length > 0 && (isDebouncing || chunkResults.some((r) => r.isLoading));

  return { thumbnails: urlMap, isLoading };
}
