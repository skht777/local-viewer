// バッチサムネイル API フック
// - node_ids を 50 件チャンクに分割して POST /api/thumbnails/batch を並列リクエスト
// - TanStack Query の useQueries でチャンク別キャッシュ・リトライ・dedup を活用
// - Query キャッシュには raw base64 data のみ、Blob URL はローカルで差分管理

import { useQueries } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { apiPost } from "./apiClient";

// 安定チャンク分割の状態
export interface ChunkState {
  chunks: string[][];
  idSet: Set<string>;
}

// 安定チャンク分割: 追加のみなら既存チャンクを維持し、新規 ID だけ新チャンクに
// タブ切替等で ID が削除された場合は全チャンク再構成する
export function computeStableChunks(ids: string[], size: number, prev: ChunkState): ChunkState {
  const currentSet = new Set(ids);

  // ID が削除された（タブ切替等）or 初回 → 全チャンク再構成
  const hasRemoved = prev.chunks.length > 0 && [...prev.idSet].some((id) => !currentSet.has(id));
  if (hasRemoved || prev.chunks.length === 0) {
    return { chunks: splitIntoChunks(ids, size), idSet: currentSet };
  }

  // 無限スクロール (追加のみ) → 既存チャンク維持 + 新規チャンク追加
  const newIds = ids.filter((id) => !prev.idSet.has(id));
  if (newIds.length === 0) return prev;
  return {
    chunks: [...prev.chunks, ...splitIntoChunks(newIds, size)],
    idSet: currentSet,
  };
}

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
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
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

/**
 * バッチサムネイル取得フック
 *
 * node_ids を 50 件チャンクに分割して並列バッチリクエストを発行し、
 * node_id → Blob URL のマップを返す。
 * Blob URL は差分管理: 共通 ID は再利用、不要分のみ revoke。
 */
export function useBatchThumbnails(nodeIds: string[]): {
  thumbnails: Map<string, string>;
  isLoading: boolean;
} {
  // デバウンス: 短時間の連続変更をまとめる (タブ切替・フィルタ変更対応)
  const [debouncedIds, setDebouncedIds] = useState(nodeIds);
  const key = useMemo(() => nodeIds.join(","), [nodeIds]);
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedIds(nodeIds), 50);
    return () => clearTimeout(timer);
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps

  // 安定チャンク分割: 追加のみなら既存チャンクを維持し、新規 ID だけ新チャンクに
  const chunksRef = useRef<ChunkState>({ chunks: [], idSet: new Set() });
  const chunks = useMemo(() => {
    const result = computeStableChunks(debouncedIds, BATCH_SIZE, chunksRef.current);
    chunksRef.current = result;
    return result.chunks;
  }, [debouncedIds]);

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

  // 全チャンク結果をマージ (dataUpdatedAt で実際のデータ変更を追跡)
  const dataKey = chunkResults.map((r) => r.dataUpdatedAt).join(",");
  const rawData = useMemo(() => {
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dataKey]);

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
