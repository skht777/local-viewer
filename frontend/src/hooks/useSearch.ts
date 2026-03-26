// キーワード検索フック
// - 入力値のデバウンス (300ms)
// - kind フィルタ状態管理
// - 503 (インデックス構築中) のハンドリング

import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import type { SearchResult } from "../types/api";
import { ApiError } from "./api/apiClient";
import { searchOptions } from "./api/browseQueries";

const DEBOUNCE_MS = 300;

interface UseSearchReturn {
  query: string;
  setQuery: (q: string) => void;
  debouncedQuery: string;
  kind: string | null;
  setKind: (k: string | null) => void;
  results: SearchResult[];
  hasMore: boolean;
  isLoading: boolean;
  isError: boolean;
  isIndexing: boolean;
}

export function useSearch(): UseSearchReturn {
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [kind, setKind] = useState<string | null>(null);

  // デバウンス
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedQuery(query);
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  const opts = searchOptions(debouncedQuery, kind ?? undefined);
  const { data, isLoading, isError, error } = useQuery({
    ...opts,
    retry: false,
  });

  // 503 → インデックス構築中
  const isIndexing = isError && error instanceof ApiError && error.status === 503;

  const resetKind = useCallback((k: string | null) => {
    setKind(k);
  }, []);

  return {
    query,
    setQuery,
    debouncedQuery,
    kind,
    setKind: resetKind,
    results: data?.results ?? [],
    hasMore: data?.has_more ?? false,
    isLoading,
    isError: isError && !isIndexing,
    isIndexing,
  };
}
