// 検索結果ページのデータレイヤー
// - URL searchParams から q / scope / kind / sort を正規化
// - searchInfiniteOptions で無限スクロールクエリを実行
// - browseNodeOptions(scope) で scope ディレクトリ名を解決
// - 結果を BrowseEntry[] に変換して返す

import { useMemo } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { browseNodeOptions, searchInfiniteOptions } from "./api/browseQueries";
import type { SearchSort } from "./api/browseQueries";
import { searchResultToBrowseEntry } from "../utils/searchResultToBrowseEntry";
import type { BrowseEntry } from "../types/api";

const VALID_SEARCH_SORTS = new Set<string>([
  "relevance",
  "name-asc",
  "name-desc",
  "date-asc",
  "date-desc",
]);

const VALID_KINDS = new Set<string>(["directory", "image", "video", "pdf", "archive"]);

export interface SearchResultsData {
  q: string;
  scope: string | null;
  kind: string | null;
  sort: SearchSort;
  isLoading: boolean;
  hasNextPage: boolean;
  fetchNextPage: () => void;
  isFetchingNextPage: boolean;
  isError: boolean;
  allEntries: BrowseEntry[];
  scopeName: string | null;
}

export function useSearchResultsData(): SearchResultsData {
  const [searchParams] = useSearchParams();

  const q = (searchParams.get("q") ?? "").trim();
  const scope = searchParams.get("scope") ?? null;
  const rawKind = searchParams.get("kind");
  const kind = rawKind && VALID_KINDS.has(rawKind) ? rawKind : null;
  const rawSort = searchParams.get("sort");
  const sort = (rawSort && VALID_SEARCH_SORTS.has(rawSort) ? rawSort : "relevance") as SearchSort;

  // scope 配下の場合、ディレクトリ名を表示するために browseNodeOptions で取得
  const { data: scopeData } = useQuery(browseNodeOptions(scope ?? undefined));

  // 検索結果（無限スクロール）
  const { data, isLoading, hasNextPage, fetchNextPage, isFetchingNextPage, isError } =
    useInfiniteQuery(searchInfiniteOptions({ q, scope, kind, sort }));

  // 検索結果を BrowseEntry に変換
  const allEntries = useMemo<BrowseEntry[]>(() => {
    if (!data?.pages?.length) {
      return [];
    }
    return data.pages.flatMap((p) => p.results.map(searchResultToBrowseEntry));
  }, [data]);

  return {
    q,
    scope,
    kind,
    sort,
    isLoading,
    hasNextPage: hasNextPage ?? false,
    fetchNextPage,
    isFetchingNextPage,
    isError,
    allEntries,
    scopeName: scopeData?.current_name ?? null,
  };
}
