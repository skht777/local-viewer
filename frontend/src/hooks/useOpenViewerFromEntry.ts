// ディレクトリ/アーカイブから再帰探索してビューワーを開く
// - resolveFirstViewable で最初の閲覧対象を探索
// - PDF / 画像 / アーカイブの種別に応じて適切な URL で遷移
// - 起点を viewerOrigin に保存し、閉じたときに元のディレクトリに復帰
// - startViewerTransition でトランジション中のブラウズ画面レンダリングを抑制
// - 画像/アーカイブは fetchAllBrowsePages で全ページ先読み（100 件超の兄弟画像対応）
// - PDF は prefetchInfiniteQuery で 1 ページ先読み（PDF 本体は別途読み込まれる）
// - 閲覧対象が見つからない場合はディレクトリ進入にフォールバック

import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { browseInfiniteOptions, fetchAllBrowsePages } from "./api/browseQueries";
import { resolveFirstViewable } from "../utils/resolveFirstViewable";
import { useViewerStore } from "../stores/viewerStore";
import { buildOpenPdfSearch } from "../utils/viewerNavigation";
import type { SortOrder, ViewerMode } from "./useViewerParams";

interface UseOpenViewerFromEntryProps {
  nodeId: string | undefined;
  mode: ViewerMode;
  sort: SortOrder;
  buildBrowseSearch: (overrides?: { tab?: string; index?: number }) => string;
  /**
   * viewer 起動時に保存する起点情報を上書きするビルダー。
   * - 未指定: `nodeId` から `/browse/${nodeId}` 形を組み立てる（既存挙動）
   * - 指定 + 戻り値が `null`: 起点保存をスキップ
   * - 指定 + 戻り値があり: その値を `setViewerOrigin` に渡す
   *
   * `/search` 等、`nodeId` を持たないページからの起動でも
   * 適切な閉じ戻り先を保存できるようにするために追加。
   */
  buildOrigin?: () => { pathname: string; search: string } | null;
}

export function useOpenViewerFromEntry({
  nodeId,
  mode: _mode,
  sort,
  buildBrowseSearch,
  buildOrigin,
}: UseOpenViewerFromEntryProps): (entryNodeId: string) => Promise<void> {
  // Mode は現時点では URL 構築側 (buildBrowseSearch) が担うため直接参照しない。
  // Props として維持するのはコール側の型互換性のため。
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
        // - buildOrigin 指定時はその戻り値を優先（null なら保存スキップ）
        // - 未指定 + nodeId あり時のみ既存の /browse/${nodeId} 形を保存
        // first-viewable 解決成功後にのみ保存することで、fallback 経路で
        // stale origin が残らないことを保証する
        const origin = buildOrigin
          ? buildOrigin()
          : nodeId
            ? { pathname: `/browse/${nodeId}`, search: buildBrowseSearch() }
            : null;
        if (origin) {
          setViewerOrigin(origin);
        }
        // トランジション開始（ブラウズ画面の不要レンダリング抑制）
        startViewerTransition();

        if (target.entry.kind === "pdf") {
          // PDF: プリフェッチ → 親ディレクトリで PDF ビューワーを開く
          // Browse スコープ (mode/sort) を維持しつつ viewerNavigation で pdf/page を付与
          // Push モード: ブラウザバックで呼び出し元に戻れるようにする（B キー閉じと一致）
          await queryClient.prefetchInfiniteQuery(browseInfiniteOptions(target.parentNodeId, sort));
          const browseBase = new URLSearchParams(buildBrowseSearch().replace(/^\?/, ""));
          const withPdf = buildOpenPdfSearch(browseBase, { pdfNodeId: target.entry.node_id });
          const searchStr = withPdf.toString() ? `?${withPdf}` : "";
          navigate(`/browse/${target.parentNodeId}${searchStr}`);
        } else if (target.entry.kind === "image") {
          // 画像: 親ディレクトリの全ページをプリフェッチ → ビューワーを開く
          // 100 件超の兄弟画像が infinite query の 1 ページ目に収まらないケースに対応
          // Push モード: ブラウザバックで呼び出し元に戻れるようにする
          await fetchAllBrowsePages(queryClient, target.parentNodeId, sort);
          navigate(
            `/browse/${target.parentNodeId}${buildBrowseSearch({ index: 0, tab: "images" })}`,
          );
        } else {
          // アーカイブ: 中身の全ページをプリフェッチしてから進入
          // Push モード: ブラウザバックで呼び出し元に戻れるようにする
          await fetchAllBrowsePages(queryClient, target.entry.node_id, sort);
          navigate(
            `/browse/${target.entry.node_id}${buildBrowseSearch({ index: 0, tab: "images" })}`,
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
      buildOrigin,
      setViewerOrigin,
      startViewerTransition,
    ],
  );
}
