// セット間ジャンプの実行オーケストレーション
// - 再帰的に親を辿りマウントルートまで兄弟セットを探索
// - shouldConfirm で確認ダイアログの出し分け判定
// - NavigationPrompt の状態管理を内包
// - CgViewer / MangaViewer / PdfCgViewer / PdfMangaViewer から共通利用
// - PDF の場合は ?pdf= 付き URL で遷移 (browse 422 回避)

import { useCallback, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { apiFetch } from "./api/apiClient";
import { browseNodeOptions } from "./api/browseQueries";
import { findNextSet, findPrevSet, resolveTopLevelDir, shouldConfirm } from "./useSetNavigation";
import type { SortOrder, ViewerMode } from "./useViewerParams";
import type { AncestorEntry, BrowseEntry, BrowseResponse, SiblingResponse } from "../types/api";
import { resolveFirstViewable } from "../utils/resolveFirstViewable";
import { sortEntries } from "../utils/sortEntries";

interface UseSetJumpProps {
  currentNodeId: string | null;
  parentNodeId: string | null;
  ancestors?: AncestorEntry[];
  mode: ViewerMode;
  sort?: SortOrder;
}

interface Prompt {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
  extraConfirmKeys?: string[];
}

// 再帰探索の結果
interface SearchResult {
  target: BrowseEntry;
  levelsUp: number;
  searchDirData: BrowseResponse;
  sourceTopDir: string | null;
}

interface UseSetJumpReturn {
  goNextSet: () => void;
  goPrevSet: () => void;
  goNextSetParent: () => void;
  goPrevSetParent: () => void;
  prompt: Prompt | null;
  dismissPrompt: () => void;
}

const MAX_DEPTH = 10;

export function useSetJump({
  currentNodeId,
  parentNodeId,
  ancestors = [],
  mode,
  sort = "name-asc",
}: UseSetJumpProps): UseSetJumpReturn {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [prompt, setPrompt] = useState<Prompt | null>(null);

  const dismissPrompt = useCallback(() => setPrompt(null), []);

  // browse スコープ (mode/sort) を含む search 文字列を構築
  const buildSearch = useCallback(
    (params: Record<string, string>): string => {
      const sp = new URLSearchParams(params);
      if (mode === "manga") sp.set("mode", "manga");
      if (sort !== "name-asc") sp.set("sort", sort);
      return `?${sp}`;
    },
    [mode, sort],
  );

  // browse ノードにナビゲートする前にデータをプリフェッチ
  // キャッシュ済みなら即座に返る。未キャッシュでも navigate 前に取得完了する
  const prefetchAndNavigate = useCallback(
    async (nodeId: string, search: string) => {
      await queryClient.prefetchQuery(browseNodeOptions(nodeId));
      navigate(`/browse/${nodeId}${search}`);
    },
    [queryClient, navigate],
  );

  // 遷移先の kind に応じた URL で遷移
  // - PDF: ターゲットの親ディレクトリに留まり ?pdf= 付きで PDF ビューワーを開く
  // - アーカイブ: そのまま進入してビューワーを開く
  // - ディレクトリ: 再帰探索して最初の閲覧対象を開く
  const navigateToTarget = useCallback(
    async (target: BrowseEntry, targetParentNodeId: string | null) => {
      if (target.kind === "pdf") {
        await prefetchAndNavigate(
          targetParentNodeId ?? target.node_id,
          buildSearch({ pdf: target.node_id, page: "1" }),
        );
        return;
      }
      if (target.kind !== "directory") {
        // アーカイブ: プリフェッチしてから進入
        await prefetchAndNavigate(target.node_id, buildSearch({ tab: "images", index: "0" }));
        return;
      }
      // ディレクトリ: 再帰探索して最初の閲覧対象を開く
      try {
        const resolved = await resolveFirstViewable(target.node_id, queryClient, sort);
        if (!resolved) {
          await prefetchAndNavigate(target.node_id, buildSearch({ tab: "images", index: "0" }));
          return;
        }
        if (resolved.entry.kind === "pdf") {
          await prefetchAndNavigate(
            resolved.parentNodeId,
            buildSearch({ pdf: resolved.entry.node_id, page: "1" }),
          );
        } else if (resolved.entry.kind === "image") {
          // parentNodeId は resolveFirstViewable 内でキャッシュ済み
          navigate(`/browse/${resolved.parentNodeId}${buildSearch({ tab: "images", index: "0" })}`);
        } else {
          // アーカイブ: プリフェッチしてから進入
          await prefetchAndNavigate(
            resolved.entry.node_id,
            buildSearch({ tab: "images", index: "0" }),
          );
        }
      } catch {
        navigate(`/browse/${target.node_id}${buildSearch({ tab: "images", index: "0" })}`);
      }
    },
    [navigate, prefetchAndNavigate, buildSearch, sort, queryClient],
  );

  // 再帰的に親を辿って兄弟セットを探索
  const findSiblingRecursive = useCallback(
    async (direction: "next" | "prev"): Promise<SearchResult | null> => {
      let currentChildId = currentNodeId;
      let currentParentId = parentNodeId;
      let levelsUp = 0;
      let sourceTopDir: string | null = null;
      let isSourceResolved = false;
      const visited = new Set<string>();

      // parentNodeId が null の場合、ancestors[0] (マウントルート) を使用
      if (!currentParentId) {
        if (ancestors.length === 0 || !currentNodeId) return null;
        currentParentId = ancestors[0].node_id;
      }

      const finder = direction === "next" ? findNextSet : findPrevSet;

      while (currentParentId && levelsUp < MAX_DEPTH) {
        if (visited.has(currentParentId)) break;
        visited.add(currentParentId);

        // sibling API を優先試行 (1 クエリで次/前セットを取得)
        let sibling: BrowseEntry | null = null;
        let parentData: BrowseResponse | null = null;
        if (currentChildId) {
          try {
            const resp = await apiFetch<SiblingResponse>(
              `/api/browse/${currentParentId}/sibling?current=${currentChildId}&direction=${direction}&sort=${sort}`,
            );
            sibling = resp.entry;
          } catch {
            // API 失敗時はフォールバック
          }
        }

        // sibling API で見つからなかった or 失敗 → フォールバック: 全件取得
        if (!parentData) {
          parentData = await queryClient.fetchQuery(browseNodeOptions(currentParentId, sort));
        }
        if (!currentChildId) break;

        if (!sibling) {
          const sorted = sortEntries(parentData.entries, sort);
          sibling = finder(sorted, currentChildId);
        }

        // ソースの topDir を level 0 で算出
        if (!isSourceResolved) {
          const sourceEntry = sorted.find((e) => e.node_id === currentChildId);
          if (sourceEntry) {
            sourceTopDir = resolveTopLevelDir(
              parentData.ancestors,
              parentData.current_node_id,
              sourceEntry,
            );
          }
          isSourceResolved = true;
        }

        if (sibling) {
          return { target: sibling, levelsUp, searchDirData: parentData, sourceTopDir };
        }

        // 兄弟なし → 上に登る
        levelsUp++;
        currentChildId = parentData.current_node_id;
        currentParentId = parentData.parent_node_id;

        // parent_node_id が null → ancestors から mount root を取得
        if (!currentParentId && parentData.ancestors.length > 0) {
          currentParentId = parentData.ancestors[0].node_id;
        }
      }

      return null;
    },
    [currentNodeId, parentNodeId, ancestors, sort, queryClient],
  );

  // PageDown/X: 条件付き確認で次のセットへ
  const goNextSet = useCallback(async () => {
    const result = await findSiblingRecursive("next");
    if (!result) return;

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
  }, [findSiblingRecursive, navigateToTarget]);

  // PageUp/Z: 条件付き確認で前のセットへ
  const goPrevSet = useCallback(async () => {
    const result = await findSiblingRecursive("prev");
    if (!result) return;

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
  }, [findSiblingRecursive, navigateToTarget]);

  // Shift+X: 確認なしで次のセットへ
  const goNextSetParent = useCallback(async () => {
    const result = await findSiblingRecursive("next");
    if (result) navigateToTarget(result.target, result.searchDirData.current_node_id);
  }, [findSiblingRecursive, navigateToTarget]);

  // Shift+Z: 確認なしで前のセットへ
  const goPrevSetParent = useCallback(async () => {
    const result = await findSiblingRecursive("prev");
    if (result) navigateToTarget(result.target, result.searchDirData.current_node_id);
  }, [findSiblingRecursive, navigateToTarget]);

  return { goNextSet, goPrevSet, goNextSetParent, goPrevSetParent, prompt, dismissPrompt };
}
