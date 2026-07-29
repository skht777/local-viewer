// セット間ジャンプの実行オーケストレーション
// - 再帰的に親を辿りマウントルートまで兄弟セットを探索
// - shouldConfirm で確認ダイアログの出し分け判定
// - NavigationPrompt の状態管理を内包
// - CgViewer / MangaViewer / PdfCgViewer / PdfMangaViewer から共通利用
// - PDF の場合は ?pdf= 付き URL で遷移 (browse 422 回避)

import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { browseInfiniteOptions, fetchAllBrowsePages } from "./api/browseQueries";
import { resolveTopLevelDir, shouldConfirm } from "./useSetNavigation";
import { useFindSiblingRecursive } from "./useFindSiblingRecursive";
import type { SortOrder, ViewerMode } from "./useViewerParams";
import type { AncestorEntry } from "../types/api";
import type { SetJumpTarget } from "../lib/jumpListNavigation";
import { resolveJumpListAction } from "../lib/jumpListNavigation";
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

// jumpList 経路の判定 + 副作用ハンドラ
// - 戻り値 true: 処理済み (FS sibling 経路へフォールバックしない)
// - 戻り値 false: jumpList null のため呼び出し側で従来 FS 経路へ
// - index ベース: viewer の currentNodeId が resolveFirstViewable で着地ずれしても
//   起動 entry の位置で固定された jumpListIndex を起点に next/prev する
function useJumpListHandler(
  navigateToTarget: (target: SetJumpTarget, parent: string | null) => Promise<void>,
  onBoundary: ((message: string) => void) | undefined,
): (direction: "next" | "prev") => Promise<boolean> {
  const viewerJumpList = useViewerStore((s) => s.viewerJumpList);
  const viewerJumpListIndex = useViewerStore((s) => s.viewerJumpListIndex);
  const setViewerJumpListIndex = useViewerStore((s) => s.setViewerJumpListIndex);
  return useCallback(
    async (direction) => {
      const action = resolveJumpListAction(direction, viewerJumpListIndex, viewerJumpList);
      if (action.type === "fallback") {
        return false;
      }
      if (action.type === "boundary") {
        onBoundary?.(action.message);
        return true;
      }
      // navigate 前に index を更新（store の即時反映で連打時も整合性を保つ）
      setViewerJumpListIndex(action.nextIndex);
      await navigateToTarget(action.target, action.target.parent_node_id);
      return true;
    },
    [viewerJumpList, viewerJumpListIndex, setViewerJumpListIndex, navigateToTarget, onBoundary],
  );
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
  const cancelViewerTransition = useViewerStore((s) => s.cancelViewerTransition);
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);

  // ビューワーを閉じた (unmount) 後に完走した非同期チェーンが navigate して、
  // 閉じたはずの画面が別ディレクトリへ置き換わる/ビューワーが再オープンするのを防ぐ
  const isMountedRef = useRef(true);
  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);

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

  // PDF 用: 1 ページだけプリフェッチして navigate (replace で履歴汚染回避)
  const prefetchFirstPageAndNavigate = useCallback(
    async (nodeId: string, search: string) => {
      // 遷移先 nodeId を記録し、遷移先の data 到着まで transition を維持する
      startViewerTransition(nodeId);
      await queryClient.prefetchInfiniteQuery(browseInfiniteOptions(nodeId, sort));
      if (!isMountedRef.current) {
        cancelViewerTransition();
        return;
      }
      navigate(`/browse/${nodeId}${search}`, { replace: true });
    },
    [queryClient, navigate, sort, startViewerTransition, cancelViewerTransition],
  );

  // image / archive 用: 全ページをプリフェッチして navigate (replace、100 件超対応)
  const prefetchAllAndNavigate = useCallback(
    async (nodeId: string, search: string) => {
      startViewerTransition(nodeId);
      await fetchAllBrowsePages(queryClient, nodeId, sort);
      if (!isMountedRef.current) {
        cancelViewerTransition();
        return;
      }
      navigate(`/browse/${nodeId}${search}`, { replace: true });
    },
    [queryClient, navigate, sort, startViewerTransition, cancelViewerTransition],
  );

  // 遷移先 kind に応じた URL 遷移 (PDF=親dir+?pdf=、archive=進入、directory=first-viewable)
  // 引数 SetJumpTarget は BrowseEntry / JumpListEntry いずれも構造的に渡せる
  const navigateToTarget = useCallback(
    async (target: SetJumpTarget, targetParentNodeId: string | null) => {
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
        if (!isMountedRef.current) {
          return;
        }
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
          startViewerTransition(resolved.parentNodeId);
          await fetchAllBrowsePages(queryClient, resolved.parentNodeId, sort);
          if (!isMountedRef.current) {
            cancelViewerTransition();
            return;
          }
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
        // 開始済み transition は遷移先に着地しないため固着防止でリセット
        cancelViewerTransition();
        if (!isMountedRef.current) {
          return;
        }
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
      cancelViewerTransition,
    ],
  );

  const findSiblingRecursive = useFindSiblingRecursive({
    currentNodeId,
    parentNodeId,
    ancestors,
    sort,
  });

  const handleJumpList = useJumpListHandler(navigateToTarget, onBoundary);

  // PageDown/X: 条件付き確認で次のセットへ（トランジション中は無効化）
  const goNextSet = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    if (await handleJumpList("next")) {
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
  }, [handleJumpList, findSiblingRecursive, navigateToTarget, onBoundary, viewerTransitionId]);

  // PageUp/Z: 条件付き確認で前のセットへ（トランジション中は無効化）
  const goPrevSet = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    if (await handleJumpList("prev")) {
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
  }, [handleJumpList, findSiblingRecursive, navigateToTarget, onBoundary, viewerTransitionId]);

  // Shift+X: 確認なしで次のセットへ（トランジション中は無効化）
  const goNextSetParent = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    if (await handleJumpList("next")) {
      return;
    }
    const result = await findSiblingRecursive("next");
    if (result) {
      navigateToTarget(result.target, result.searchDirData.current_node_id);
    }
  }, [handleJumpList, findSiblingRecursive, navigateToTarget, viewerTransitionId]);

  // Shift+Z: 確認なしで前のセットへ（トランジション中は無効化）
  const goPrevSetParent = useCallback(async () => {
    if (viewerTransitionId > 0) {
      return;
    }
    if (await handleJumpList("prev")) {
      return;
    }
    const result = await findSiblingRecursive("prev");
    if (result) {
      navigateToTarget(result.target, result.searchDirData.current_node_id);
    }
  }, [handleJumpList, findSiblingRecursive, navigateToTarget, viewerTransitionId]);

  return { goNextSet, goPrevSet, goNextSetParent, goPrevSetParent, prompt, dismissPrompt };
}
