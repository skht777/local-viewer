// セット間ジャンプの実行オーケストレーション
// - 再帰的に親を辿りマウントルートまで兄弟セットを探索
// - shouldConfirm で確認ダイアログの出し分け判定
// - NavigationPrompt の状態管理を内包
// - CgViewer / MangaViewer / PdfCgViewer / PdfMangaViewer から共通利用
// - PDF の場合は ?pdf= 付き URL で遷移 (browse 422 回避)

import { useCallback, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { browseInfiniteOptions, fetchAllBrowsePages } from "./api/browseQueries";
import { resolveTopLevelDir, shouldConfirm } from "./useSetNavigation";
import { useFindSiblingRecursive } from "./useFindSiblingRecursive";
import type { SortOrder, ViewerMode } from "./useViewerParams";
import type { AncestorEntry, BrowseEntry } from "../types/api";
import { useViewerStore } from "../stores/viewerStore";
import { resolveFirstViewable } from "../utils/resolveFirstViewable";

interface UseSetJumpProps {
  currentNodeId: string | null;
  parentNodeId: string | null;
  ancestors?: AncestorEntry[];
  mode: ViewerMode;
  sort?: SortOrder;
  onBoundary?: (message: string) => void;
}

interface Prompt {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
  extraConfirmKeys?: string[];
}

interface UseSetJumpReturn {
  goNextSet: () => void;
  goPrevSet: () => void;
  goNextSetParent: () => void;
  goPrevSetParent: () => void;
  prompt: Prompt | null;
  dismissPrompt: () => void;
}

export function useSetJump({
  currentNodeId,
  parentNodeId,
  ancestors = [],
  mode,
  sort = "name-asc",
  onBoundary,
}: UseSetJumpProps): UseSetJumpReturn {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [prompt, setPrompt] = useState<Prompt | null>(null);
  const startViewerTransition = useViewerStore((s) => s.startViewerTransition);
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);

  const dismissPrompt = useCallback(() => setPrompt(null), []);

  // browse スコープ (mode/sort) を含む search 文字列を構築
  const buildSearch = useCallback(
    (params: Record<string, string>): string => {
      const sp = new URLSearchParams(params);
      if (mode === "manga") {
        sp.set("mode", "manga");
      }
      if (sort !== "name-asc") {
        sp.set("sort", sort);
      }
      return `?${sp}`;
    },
    [mode, sort],
  );

  // PDF 用: 1 ページだけプリフェッチして navigate

  // - PDF ビューワーは PDF 自身を表示するため親ディレクトリの全件は不要
  // - replace モード: セットジャンプは履歴を汚染しない
  const prefetchFirstPageAndNavigate = useCallback(
    async (nodeId: string, search: string) => {
      startViewerTransition();
      await queryClient.prefetchInfiniteQuery(browseInfiniteOptions(nodeId, sort));
      navigate(`/browse/${nodeId}${search}`, { replace: true });
    },
    [queryClient, navigate, sort, startViewerTransition],
  );

  // image / archive 用: 全ページをプリフェッチして navigate
  // - 100 件超のセットでもビューワー側に全画像が渡るよう兄弟全件を温める
  // - replace モード: セットジャンプは履歴を汚染しない
  const prefetchAllAndNavigate = useCallback(
    async (nodeId: string, search: string) => {
      startViewerTransition();
      await fetchAllBrowsePages(queryClient, nodeId, sort);
      navigate(`/browse/${nodeId}${search}`, { replace: true });
    },
    [queryClient, navigate, sort, startViewerTransition],
  );

  // 遷移先の kind に応じた URL で遷移
  // - PDF: ターゲットの親ディレクトリに留まり ?pdf= 付きで PDF ビューワーを開く
  // - アーカイブ: そのまま進入してビューワーを開く
  // - ディレクトリ: 再帰探索して最初の閲覧対象を開く
  const navigateToTarget = useCallback(
    async (target: BrowseEntry, targetParentNodeId: string | null) => {
      if (target.kind === "pdf") {
        await prefetchFirstPageAndNavigate(
          targetParentNodeId ?? target.node_id,
          buildSearch({ pdf: target.node_id, page: "1" }),
        );
        return;
      }
      if (target.kind !== "directory") {
        // アーカイブ: 全ページをプリフェッチしてから進入（100 件超でも viewer に渡す）
        await prefetchAllAndNavigate(target.node_id, buildSearch({ tab: "images", index: "0" }));
        return;
      }
      // ディレクトリ: 再帰探索して最初の閲覧対象を開く
      try {
        const resolved = await resolveFirstViewable(target.node_id, queryClient, sort);
        if (!resolved) {
          // index なしで遷移 → ブラウザーモードでコンテンツを確認（1 ページで十分）
          await prefetchFirstPageAndNavigate(target.node_id, buildSearch({ tab: "images" }));
          return;
        }
        if (resolved.entry.kind === "pdf") {
          await prefetchFirstPageAndNavigate(
            resolved.parentNodeId,
            buildSearch({ pdf: resolved.entry.node_id, page: "1" }),
          );
        } else if (resolved.entry.kind === "image") {
          // 画像: 親ディレクトリの全ページをプリフェッチしてから navigate
          // 100 件超の兄弟画像が viewer に渡るよう保証する
          startViewerTransition();
          await fetchAllBrowsePages(queryClient, resolved.parentNodeId, sort);
          navigate(
            `/browse/${resolved.parentNodeId}${buildSearch({ tab: "images", index: "0" })}`,
            { replace: true },
          );
        } else {
          // アーカイブ: 全ページをプリフェッチしてから進入
          await prefetchAllAndNavigate(
            resolved.entry.node_id,
            buildSearch({ tab: "images", index: "0" }),
          );
        }
      } catch {
        // エラー時も index なしで遷移 → ブラウザーモードでコンテンツを確認
        navigate(`/browse/${target.node_id}${buildSearch({ tab: "images" })}`, { replace: true });
      }
    },
    [
      navigate,
      prefetchFirstPageAndNavigate,
      prefetchAllAndNavigate,
      buildSearch,
      sort,
      queryClient,
      startViewerTransition,
    ],
  );

  const findSiblingRecursive = useFindSiblingRecursive({
    currentNodeId,
    parentNodeId,
    ancestors,
    sort,
  });

  // PageDown/X: 条件付き確認で次のセットへ（トランジション中は無効化）
  const goNextSet = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    const result = await findSiblingRecursive("next");
    if (!result) {
      onBoundary?.("最後のセットです");
      return;
    }

    const targetTopDir = resolveTopLevelDir(
      result.searchDirData.ancestors,
      result.searchDirData.current_node_id,
      result.target,
    );

    if (shouldConfirm(result.levelsUp, result.sourceTopDir, targetTopDir)) {
      setPrompt({
        message: "次のディレクトリに移動しますか？",
        onConfirm: () => {
          setPrompt(null);
          navigateToTarget(result.target, result.searchDirData.current_node_id);
        },
        onCancel: () => setPrompt(null),
        extraConfirmKeys: ["x"],
      });
    } else {
      navigateToTarget(result.target, result.searchDirData.current_node_id);
    }
  }, [findSiblingRecursive, navigateToTarget, onBoundary, viewerTransitionId]);

  // PageUp/Z: 条件付き確認で前のセットへ（トランジション中は無効化）
  const goPrevSet = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    const result = await findSiblingRecursive("prev");
    if (!result) {
      onBoundary?.("最初のセットです");
      return;
    }

    const targetTopDir = resolveTopLevelDir(
      result.searchDirData.ancestors,
      result.searchDirData.current_node_id,
      result.target,
    );

    if (shouldConfirm(result.levelsUp, result.sourceTopDir, targetTopDir)) {
      setPrompt({
        message: "前のディレクトリに移動しますか？",
        onConfirm: () => {
          setPrompt(null);
          navigateToTarget(result.target, result.searchDirData.current_node_id);
        },
        onCancel: () => setPrompt(null),
        extraConfirmKeys: ["z"],
      });
    } else {
      navigateToTarget(result.target, result.searchDirData.current_node_id);
    }
  }, [findSiblingRecursive, navigateToTarget, onBoundary, viewerTransitionId]);

  // Shift+X: 確認なしで次のセットへ（トランジション中は無効化）
  const goNextSetParent = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    const result = await findSiblingRecursive("next");
    if (result) {
      navigateToTarget(result.target, result.searchDirData.current_node_id);
    }
  }, [findSiblingRecursive, navigateToTarget, viewerTransitionId]);

  // Shift+Z: 確認なしで前のセットへ（トランジション中は無効化）
  const goPrevSetParent = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    const result = await findSiblingRecursive("prev");
    if (result) {
      navigateToTarget(result.target, result.searchDirData.current_node_id);
    }
  }, [findSiblingRecursive, navigateToTarget, viewerTransitionId]);

  return { goNextSet, goPrevSet, goNextSetParent, goPrevSetParent, prompt, dismissPrompt };
}
