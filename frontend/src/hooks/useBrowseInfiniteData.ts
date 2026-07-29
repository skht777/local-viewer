// BrowsePage の useInfiniteQuery と全ページ結合 + viewerTransitionId 終了の集約
// - data は先頭ページのメタ + 全ページの entries を結合した形で返す
//   (ページ跨ぎの重複 node_id は初出優先で除去する)
// - セットジャンプのトランジション完了: 「遷移先 nodeId」の data 到着で endViewerTransition を呼ぶ
//   (遷移元の data は常に到着済みのため、nodeId 判定が無いと開始直後に即解除され
//    連打ガードとビューワー unmount 抑制が機能しない)
// - 遷移先の取得エラーでも解除する (トランジションの固着防止)
// - 戻り値はそのまま BrowsePage で消費するので useInfiniteQuery 同等のフィールドを露出

import { useEffect, useMemo } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { browseInfiniteOptions } from "./api/browseQueries";
import type { SortOrder } from "./useViewerParams";
import { useViewerStore } from "../stores/viewerStore";
import { dedupeByNodeId } from "../utils/dedupeEntries";
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
  const viewerTransitionTarget = useViewerStore((s) => s.viewerTransitionTarget);
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
  // - cursor 不明時にバックエンドが先頭から返す仕様上ページ間で重複しうるため dedup する
  const data = useMemo(() => {
    if (!infiniteData?.pages?.length) {
      return undefined;
    }
    const [first] = infiniteData.pages;
    const allEntries = dedupeByNodeId(infiniteData.pages.flatMap((p) => p.entries));
    return {
      ...first,
      entries: allEntries,
    };
  }, [infiniteData]);

  // セットジャンプのトランジション完了: 遷移先のデータ到着 (またはエラー) でクリア
  useEffect(() => {
    if (viewerTransitionId === 0 || nodeId === undefined || nodeId !== viewerTransitionTarget) {
      return;
    }
    if ((data && !isLoading) || isError) {
      endViewerTransition(viewerTransitionId);
    }
  }, [
    viewerTransitionId,
    viewerTransitionTarget,
    nodeId,
    data,
    isLoading,
    isError,
    endViewerTransition,
  ]);

  return {
    data,
    isLoading,
    hasNextPage: hasNextPage ?? false,
    fetchNextPage: () => fetchNextPage(),
    isFetchingNextPage,
    isError,
  };
}
