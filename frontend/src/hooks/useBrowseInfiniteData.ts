// BrowsePage の useInfiniteQuery と全ページ結合 + viewerTransitionId 終了の集約
// - data は先頭ページのメタ + 全ページの entries を flatMap した形で返す
// - セットジャンプのトランジション完了: data 到着で endViewerTransition を呼ぶ
// - 戻り値はそのまま BrowsePage で消費するので useInfiniteQuery 同等のフィールドを露出

import { useEffect, useMemo } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { browseInfiniteOptions } from "./api/browseQueries";
import type { SortOrder } from "./useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import type { BrowseResponse } from "../types/api";

interface UseBrowseInfiniteDataResult {
  data: BrowseResponse | undefined;
  isLoading: boolean;
  hasNextPage: boolean;
  fetchNextPage: () => void;
  isFetchingNextPage: boolean;
  isError: boolean;
}

export function useBrowseInfiniteData(
  nodeId: string | undefined,
  sort: SortOrder,
): UseBrowseInfiniteDataResult {
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const endViewerTransition = useViewerStore((s) => s.endViewerTransition);

  const {
    data: infiniteData,
    isLoading,
    hasNextPage,
    fetchNextPage,
    isFetchingNextPage,
    isError,
  } = useInfiniteQuery(browseInfiniteOptions(nodeId, sort));

  // 全ページの entries を結合し、メタデータは先頭ページから取得
  const data = useMemo(() => {
    if (!infiniteData?.pages?.length) {
      return undefined;
    }
    const [first] = infiniteData.pages;
    const allEntries = infiniteData.pages.flatMap((p) => p.entries);
    return {
      ...first,
      entries: allEntries,
    };
  }, [infiniteData]);

  // セットジャンプのトランジション完了: データ到着でクリア
  useEffect(() => {
    if (viewerTransitionId > 0 && data && !isLoading) {
      endViewerTransition(viewerTransitionId);
    }
  }, [viewerTransitionId, data, isLoading, endViewerTransition]);

  return {
    data,
    isLoading,
    hasNextPage: hasNextPage ?? false,
    fetchNextPage: () => fetchNextPage(),
    isFetchingNextPage,
    isError,
  };
}
