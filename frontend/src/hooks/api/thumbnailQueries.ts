// バッチサムネイル API フック
// - node_ids を 50 件チャンクに分割して POST /api/thumbnails/batch を並列リクエスト
// - TanStack Query の useQueries でチャンク別キャッシュ・リトライ・dedup を活用
// - Query キャッシュには raw base64 data のみ、Blob URL はローカルで差分管理

import { useQueries } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { areNodeIdsEqual, useDebouncedValue } from "../useDebouncedValue";
import { useMergedThumbnailData } from "../useMergedThumbnailData";
import { apiPost } from "./apiClient";

// 安定チャンク分割の状態
export interface ChunkState {
  chunks: string[][];
  idSet: Set<string>;
}

// 安定チャンク分割: チャンク境界 (= queryKey) を極力変えずに維持する
// - queryKey はチャンクの ID 集合から導出するため、境界が変わると取得済みサムネイルを
//   別キーで再取得してしまう。タブ切替 (フィルタで ID が減る) でも境界を変えない
// - 既存チャンクは「現在必要な ID を 1 つも含まない」場合のみ破棄する
// - 新規 ID (無限スクロール等) は末尾に新チャンクとして追加する
// - 既存 ID が 1 つも残らない場合 (= ディレクトリ遷移) だけ全チャンク再構成する
// - idSet は「いずれかのチャンクに含まれる ID 全体」を表す
export function computeStableChunks(ids: string[], size: number, prev: ChunkState): ChunkState {
  // 初回 or 全 ID 入替 → 全チャンク再構成
  if (prev.chunks.length === 0 || !ids.some((id) => prev.idSet.has(id))) {
    return { chunks: splitIntoChunks(ids, size), idSet: new Set(ids) };
  }

  const currentSet = new Set(ids);
  const keptChunks = prev.chunks.filter((chunk) => chunk.some((id) => currentSet.has(id)));
  const knownIds = new Set(keptChunks.flat());
  const newIds = ids.filter((id) => !knownIds.has(id));

  if (newIds.length === 0 && keptChunks.length === prev.chunks.length) {
    return prev;
  }
  for (const id of newIds) {
    knownIds.add(id);
  }
  return {
    chunks: [...keptChunks, ...splitIntoChunks(newIds, size)],
    idSet: knownIds,
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

// browse API の page size (100) と揃える
const BATCH_SIZE = 100;

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

interface BlobUrlEntry {
  base64: string;
  url: string;
}

// 参照安定な空マップ (setState を no-op にして無駄な再レンダリングを避ける)
const EMPTY_URL_MAP = new Map<string, string>();

// rawData (node_id → base64) を Blob URL マップへ差分反映する純粋ロジック
// - 内容が同じ ID は既存 URL を再利用し、変化した / 不要になった分だけ revoke する
//   (refetch でサムネイルが更新されると base64 が変わるため、node_id の一致だけでは
//    古い画像を返し続けてしまう)
// - wantedIds に無い ID は表示対象外なので URL を持たない。チャンク境界を安定化した
//   結果、rawData には現在表示していない ID が残りうるため明示的に除外する
function syncBlobUrls(
  rawData: Map<string, string>,
  wantedIds: Set<string>,
  prev: Map<string, BlobUrlEntry>,
): { entries: Map<string, BlobUrlEntry>; urls: Map<string, string> } {
  const entries = new Map<string, BlobUrlEntry>();
  const urls = new Map<string, string>();
  // 表示対象の ID だけを走査する (rawData には表示対象外の ID も含まれうる)
  for (const id of wantedIds) {
    const base64 = rawData.get(id);
    if (base64 !== undefined) {
      const existing = prev.get(id);
      if (existing?.base64 === base64) {
        entries.set(id, existing);
        urls.set(id, existing.url);
      } else {
        if (existing) {
          URL.revokeObjectURL(existing.url);
        }
        const url = base64ToBlobUrl(base64);
        entries.set(id, { base64, url });
        urls.set(id, url);
      }
    }
  }
  // 不要になった URL のみ revoke
  for (const [id, entry] of prev) {
    if (!entries.has(id)) {
      URL.revokeObjectURL(entry.url);
    }
  }
  return { entries, urls };
}

// Blob URL のライフサイクルを useEffect に閉じ込める
// - createObjectURL / revokeObjectURL / ref 更新は副作用なので useMemo では扱わない
//   (レンダリングが破棄されると URL がリークする)
// - アンマウント時のクリーンアップで ref も空にするため、StrictMode の
//   マウント → クリーンアップ → 再マウントでも revoke 済み URL を再利用しない
function useThumbnailBlobUrls(
  rawData: Map<string, string>,
  wantedIds: Set<string>,
): Map<string, string> {
  const entriesRef = useRef(new Map<string, BlobUrlEntry>());
  const [urlMap, setUrlMap] = useState<Map<string, string>>(EMPTY_URL_MAP);

  useEffect(() => {
    if (rawData.size === 0) {
      setUrlMap(EMPTY_URL_MAP);
      return;
    }
    const { entries, urls } = syncBlobUrls(rawData, wantedIds, entriesRef.current);
    entriesRef.current = entries;
    setUrlMap(urls);
  }, [rawData, wantedIds]);

  // アンマウント時に全 Blob URL を解放
  // oxlint-disable-next-line arrow-body-style
  useEffect(() => {
    return () => {
      for (const entry of entriesRef.current.values()) {
        URL.revokeObjectURL(entry.url);
      }
      entriesRef.current = new Map();
    };
  }, []);

  return urlMap;
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
  // areNodeIdsEqual で配列の構造同一性を判定し、参照変更のみのケースを除外する。
  const debouncedIds = useDebouncedValue(nodeIds, 50, areNodeIdsEqual);

  // 安定チャンク分割: 追加のみなら既存チャンクを維持し、新規 ID だけ新チャンクに
  const chunksRef = useRef<ChunkState>({ chunks: [], idSet: new Set() });
  const chunks = useMemo(() => {
    const result = computeStableChunks(debouncedIds, BATCH_SIZE, chunksRef.current);
    chunksRef.current = result;
    return result.chunks;
  }, [debouncedIds]);

  // useQueries: チャンク別に並列バッチリクエスト
  // - queryKey にはソート済み ID を使用 → 表示順が変わってもキャッシュヒット
  // - signal を apiPost に渡し、キー変更/アンマウント時にリクエストを中断できるようにする
  const chunkResults = useQueries({
    queries: chunks.map((chunk) => ({
      queryKey: ["thumbnails", "batch", chunk.toSorted().join(",")],
      queryFn: async ({ signal }: { signal: AbortSignal }) => {
        const resp = await apiPost<BatchResponse>(
          "/api/thumbnails/batch",
          { node_ids: chunk },
          signal,
        );
        return resp;
      },
      enabled: chunk.length > 0,
      staleTime: 10 * 60 * 1000,
    })),
  });

  // 全チャンク結果をマージ: dataUpdatedAt シグナルで memoize する責務は useMergedThumbnailData に委譲
  const rawData = useMergedThumbnailData(chunkResults);

  // 表示対象の ID 集合 (チャンクには表示対象外の ID が残りうるため URL 生成前に絞る)
  const wantedIds = useMemo(() => new Set(debouncedIds), [debouncedIds]);

  // Blob URL の差分管理 (内容が同じ ID は再利用、変化した/不要な分は revoke)
  const urlMap = useThumbnailBlobUrls(rawData, wantedIds);

  // ローディング状態: デバウンス待ちまたはチャンク取得中
  const isDebouncing = !areNodeIdsEqual(nodeIds, debouncedIds);
  const isLoading = nodeIds.length > 0 && (isDebouncing || chunkResults.some((r) => r.isLoading));

  return { thumbnails: urlMap, isLoading };
}
