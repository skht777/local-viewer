// Browse API の TanStack Query オプション定義

import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";
import type { SortOrder } from "../../hooks/useViewerParams";
import type { BrowseResponse, SearchResponse } from "../../types/api";
import { dedupeByNodeId } from "../../utils/dedupeEntries";
import { apiFetch } from "./apiClient";

// ページネーション対応の無限スクロールクエリ
const PAGE_SIZE = 100;

export function browseInfiniteOptions(nodeId: string | undefined, sort: SortOrder) {
  return infiniteQueryOptions({
    enabled: Boolean(nodeId),
    getNextPageParam: (lastPage: BrowseResponse) => lastPage.next_cursor ?? undefined,
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams({
        limit: String(PAGE_SIZE),
        sort,
      });
      if (pageParam) {
        params.set("cursor", pageParam);
      }
      return apiFetch<BrowseResponse>(`/api/browse/${nodeId}?${params.toString()}`);
    },
    queryKey: ["browse-infinite", nodeId, sort],
  });
}

// ビューワー起動時の全ページプリフェッチ上限（= 最大 20,000 エントリ）
// `fetchInfiniteQuery` の `pages` に渡すと、`getNextPageParam` が undefined を返した時点で
// ライブラリ側の do-while ループが自発的に break する。安全上限として機能。
export const MAX_PAGES = 200;

// BrowseInfiniteOptions の全ページを逐次フェッチしてキャッシュに詰める
// - ビューワー起動時に兄弟画像が 100 件で打ち切られないようにするためのヘルパー
// - `prefetchInfiniteQuery` は例外を握り潰すため、呼び元の try/catch で拾える
//   `fetchInfiniteQuery` を使う
// - `staleTime: 0` 強制: QueryClient 既定 staleTime 内に `useSiblingPrefetch`
//   が 1 ページ目だけ入れた fresh cache があっても、page 2..N まで取得し直す
// - 完了済みキャッシュ（最終ページの `next_cursor` が null）は早期リターン
// - 上限到達（最終ページに `next_cursor` が残っている）は警告のみで続行
export async function fetchAllBrowsePages(
  queryClient: QueryClient,
  nodeId: string,
  sort: SortOrder,
): Promise<void> {
  const options = browseInfiniteOptions(nodeId, sort);
  const cached = queryClient.getQueryData<{ pages: BrowseResponse[] }>(options.queryKey);
  if (cached?.pages?.length && cached.pages.at(-1)?.next_cursor === null) {
    return;
  }
  const result = await queryClient.fetchInfiniteQuery({
    ...options,
    pages: MAX_PAGES,
    staleTime: 0,
  });
  const lastPage = result.pages.at(-1);
  if (result.pages.length >= MAX_PAGES && lastPage?.next_cursor) {
    console.warn(
      `fetchAllBrowsePages: MAX_PAGES (${MAX_PAGES}) に到達しました。nodeId=${nodeId} の続きのページは取得されていません`,
    );
  }
}

// 全ページ結合済みの BrowseResponse を返す（クライアント側の全件探索用）
// - 兄弟セット探索 / first-viewable のフォールバックのように「全エントリ」が要るケース専用
// - limit 無しの `browse` キーを廃止し、キャッシュを `browse-infinite` に一本化する。
//   これにより BrowsePage が既に読み込んだページをそのまま再利用できる
// - 全ページ取得済みのキャッシュがあればリクエストを発行しない
// - 取得失敗時は例外を送出する（呼び出し側の try/catch に委ねる）
export async function fetchBrowseAllEntries(
  queryClient: QueryClient,
  nodeId: string,
  sort: SortOrder,
): Promise<BrowseResponse | null> {
  await fetchAllBrowsePages(queryClient, nodeId, sort);
  const cached = queryClient.getQueryData<{ pages: BrowseResponse[] }>(
    browseInfiniteOptions(nodeId, sort).queryKey,
  );
  const first = cached?.pages?.[0];
  if (!first) {
    return null;
  }
  return { ...first, entries: dedupeByNodeId(cached.pages.flatMap((p) => p.entries)) };
}

// キーワード検索
// Scope: ディレクトリスコープの node_id（指定ディレクトリ配下のみ検索）
export function searchOptions(query: string, kind?: string, scope?: string) {
  const params = new URLSearchParams({ limit: "50", q: query });
  if (kind) {
    params.set("kind", kind);
  }
  if (scope) {
    params.set("scope", scope);
  }
  return queryOptions({
    enabled: query.length >= 2,
    queryFn: () => apiFetch<SearchResponse>(`/api/search?${params}`),
    queryKey: ["search", query, kind, scope],
  });
}

// 検索結果ページ用の検索ソート種別
export type SearchSort = "relevance" | "name-asc" | "name-desc" | "date-asc" | "date-desc";

// 検索結果の無限スクロールクエリ
// QueryKey は q.trim() / scope ?? null / kind ?? "all" / sort ?? "relevance" で正規化
// バックエンドは offset ベースのページネーションだが、next_offset で TanStack の cursor 互換に変換する
const SEARCH_PAGE_SIZE = 50;

export interface SearchInfiniteParams {
  q: string;
  scope?: string | null;
  kind?: string | null;
  sort?: SearchSort | null;
}

export function searchInfiniteOptions({ q, scope, kind, sort }: SearchInfiniteParams) {
  // キャッシュキー正規化
  const normQ = q.trim();
  const normScope = scope ?? null;
  const normKind = kind ?? "all";
  const normSort: SearchSort = sort ?? "relevance";

  return infiniteQueryOptions({
    enabled: normQ.length >= 2,
    getNextPageParam: (lastPage: SearchResponse) => lastPage.next_offset ?? undefined,
    initialPageParam: 0 as number,
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams({
        q: normQ,
        limit: String(SEARCH_PAGE_SIZE),
        offset: String(pageParam ?? 0),
      });
      if (normKind !== "all") {
        params.set("kind", normKind);
      }
      if (normScope) {
        params.set("scope", normScope);
      }
      if (normSort !== "relevance") {
        params.set("sort", normSort);
      }
      return apiFetch<SearchResponse>(`/api/search?${params.toString()}`);
    },
    queryKey: ["search-infinite", normQ, normScope, normKind, normSort] as const,
  });
}
