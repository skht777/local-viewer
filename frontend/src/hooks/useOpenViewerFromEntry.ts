// ディレクトリ/アーカイブから再帰探索してビューワーを開く
// - resolveFirstViewable で最初の閲覧対象を探索
// - PDF / 画像 / アーカイブの種別に応じて適切な URL で遷移
// - 起点を viewerOrigin に保存し、閉じたときに元のディレクトリに復帰
// - startViewerTransition でトランジション中のブラウズ画面レンダリングを抑制
// - prefetchInfiniteQuery でデータを先読みし、ビューワー表示を高速化
// - 閲覧対象が見つからない場合はディレクトリ進入にフォールバック

import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { browseInfiniteOptions } from "./api/browseQueries";
import { resolveFirstViewable } from "../utils/resolveFirstViewable";
import { useViewerStore } from "../stores/viewerStore";
import { buildOpenPdfSearch } from "../utils/viewerNavigation";
import type { SortOrder, ViewerMode } from "./useViewerParams";

interface UseOpenViewerFromEntryProps {
  nodeId: string | undefined;
  mode: ViewerMode;
  sort: SortOrder;
  buildBrowseSearch: (overrides?: { tab?: string; index?: number }) => string;
}

export function useOpenViewerFromEntry({
  nodeId,
  mode: _mode,
  sort,
  buildBrowseSearch,
}: UseOpenViewerFromEntryProps): (entryNodeId: string) => Promise<void> {
  // mode は現時点では URL 構築側 (buildBrowseSearch) が担うため直接参照しない。
  // props として維持するのはコール側の型互換性のため。
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const setViewerOrigin = useViewerStore((s) => s.setViewerOrigin);
  const startViewerTransition = useViewerStore((s) => s.startViewerTransition);

  return useCallback(
    async (entryNodeId: string) => {
      try {
        const target = await resolveFirstViewable(entryNodeId, queryClient, sort);
        if (!target) {
          navigate(`/browse/${entryNodeId}${buildBrowseSearch()}`);
          return;
        }

        // 起点記録（閉じる時に戻る先）
        if (nodeId) {
          setViewerOrigin({ nodeId, search: buildBrowseSearch() });
        }
        // トランジション開始（ブラウズ画面の不要レンダリング抑制）
        startViewerTransition();

        if (target.entry.kind === "pdf") {
          // PDF: プリフェッチ → 親ディレクトリで PDF ビューワーを開く
          // browse スコープ (mode/sort) を維持しつつ viewerNavigation で pdf/page を付与
          await queryClient.prefetchInfiniteQuery(browseInfiniteOptions(target.parentNodeId, sort));
          const browseBase = new URLSearchParams(buildBrowseSearch().replace(/^\?/, ""));
          const withPdf = buildOpenPdfSearch(browseBase, { pdfNodeId: target.entry.node_id });
          const searchStr = withPdf.toString() ? `?${withPdf}` : "";
          navigate(`/browse/${target.parentNodeId}${searchStr}`, { replace: true });
        } else if (target.entry.kind === "image") {
          // 画像: プリフェッチ → 親ディレクトリでビューワーを開く
          await queryClient.prefetchInfiniteQuery(browseInfiniteOptions(target.parentNodeId, sort));
          navigate(
            `/browse/${target.parentNodeId}${buildBrowseSearch({ tab: "images", index: 0 })}`,
            { replace: true },
          );
        } else {
          // アーカイブ: プリフェッチしてから進入
          await queryClient.prefetchInfiniteQuery(
            browseInfiniteOptions(target.entry.node_id, sort),
          );
          navigate(
            `/browse/${target.entry.node_id}${buildBrowseSearch({ tab: "images", index: 0 })}`,
            { replace: true },
          );
        }
      } catch {
        // エラー時は進入にフォールバック
        navigate(`/browse/${entryNodeId}${buildBrowseSearch()}`);
      }
    },
    [
      navigate,
      queryClient,
      nodeId,
      sort,
      buildBrowseSearch,
      setViewerOrigin,
      startViewerTransition,
    ],
  );
}
