// ディレクトリ/アーカイブから再帰探索してビューワーを開く
// - resolveFirstViewable で最初の閲覧対象を探索
// - PDF / 画像 / アーカイブの種別に応じて適切な URL で遷移
// - 閲覧対象が見つからない場合はディレクトリ進入にフォールバック

import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { browseInfiniteOptions } from "./api/browseQueries";
import { resolveFirstViewable } from "../utils/resolveFirstViewable";
import type { SortOrder, ViewerMode } from "./useViewerParams";

interface UseOpenViewerFromEntryProps {
  mode: ViewerMode;
  sort: SortOrder;
  buildBrowseSearch: (overrides?: { tab?: string; index?: number }) => string;
}

export function useOpenViewerFromEntry({
  mode,
  sort,
  buildBrowseSearch,
}: UseOpenViewerFromEntryProps): (entryNodeId: string) => Promise<void> {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  return useCallback(
    async (entryNodeId: string) => {
      try {
        const target = await resolveFirstViewable(entryNodeId, queryClient, sort);
        if (!target) {
          navigate(`/browse/${entryNodeId}${buildBrowseSearch()}`);
          return;
        }

        if (target.entry.kind === "pdf") {
          // PDF: 親ディレクトリで PDF ビューワーを開く (browse スコープを保持)
          const sp = new URLSearchParams();
          sp.set("pdf", target.entry.node_id);
          sp.set("page", "1");
          if (mode === "manga") sp.set("mode", "manga");
          if (sort !== "name-asc") sp.set("sort", sort);
          navigate(`/browse/${target.parentNodeId}?${sp}`);
        } else if (target.entry.kind === "image") {
          // 画像: 親ディレクトリでビューワーを開く
          navigate(
            `/browse/${target.parentNodeId}${buildBrowseSearch({ tab: "images", index: 0 })}`,
          );
        } else {
          // アーカイブ: BrowsePage の infinite キャッシュにプリフェッチしてから進入
          await queryClient.prefetchInfiniteQuery(
            browseInfiniteOptions(target.entry.node_id, sort),
          );
          navigate(
            `/browse/${target.entry.node_id}${buildBrowseSearch({ tab: "images", index: 0 })}`,
          );
        }
      } catch {
        // エラー時は進入にフォールバック
        navigate(`/browse/${entryNodeId}${buildBrowseSearch()}`);
      }
    },
    [navigate, queryClient, mode, sort, buildBrowseSearch],
  );
}
