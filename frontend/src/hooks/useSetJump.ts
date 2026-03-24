// セット間ジャンプの実行オーケストレーション
// - findNextSet/findPrevSet（純粋関数）で同階層の次/前を探索
// - 候補がなければ親ディレクトリの browse API を呼んで兄弟を走査
// - NavigationPrompt の状態管理を内包
// - CgViewer / MangaViewer 両方から共通利用

import { useCallback, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { browseNodeOptions } from "./api/browseQueries";
import { findNextSet, findPrevSet } from "./useSetNavigation";
import type { ViewerMode } from "./useViewerParams";

interface UseSetJumpProps {
  currentNodeId: string | null;
  parentNodeId: string | null;
  mode: ViewerMode;
}

interface Prompt {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
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
  mode,
}: UseSetJumpProps): UseSetJumpReturn {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [prompt, setPrompt] = useState<Prompt | null>(null);

  const dismissPrompt = useCallback(() => setPrompt(null), []);

  // 指定ノードへ遷移（mode を維持）
  const navigateToSet = useCallback(
    (nodeId: string) => {
      navigate(`/browse/${nodeId}?tab=images&index=0&mode=${mode}`);
    },
    [navigate, mode],
  );

  // 親の browse データから兄弟を検索
  const findSibling = useCallback(
    async (direction: "next" | "prev") => {
      if (!parentNodeId || !currentNodeId) return null;
      const parentData = await queryClient.fetchQuery(browseNodeOptions(parentNodeId));
      const finder = direction === "next" ? findNextSet : findPrevSet;
      return finder(parentData.entries, currentNodeId);
    },
    [parentNodeId, currentNodeId, queryClient],
  );

  // PageDown/X: 確認ダイアログ付きで次のセットへ
  const goNextSet = useCallback(async () => {
    const sibling = await findSibling("next");
    if (!sibling) return;
    setPrompt({
      message: "次のディレクトリに移動しますか？",
      onConfirm: () => {
        setPrompt(null);
        navigateToSet(sibling.node_id);
      },
      onCancel: () => setPrompt(null),
    });
  }, [findSibling, navigateToSet]);

  // PageUp/Z: 確認ダイアログ付きで前のセットへ
  const goPrevSet = useCallback(async () => {
    const sibling = await findSibling("prev");
    if (!sibling) return;
    setPrompt({
      message: "前のディレクトリに移動しますか？",
      onConfirm: () => {
        setPrompt(null);
        navigateToSet(sibling.node_id);
      },
      onCancel: () => setPrompt(null),
    });
  }, [findSibling, navigateToSet]);

  // Shift+X: 確認なしで次のセットへ
  const goNextSetParent = useCallback(async () => {
    const sibling = await findSibling("next");
    if (sibling) navigateToSet(sibling.node_id);
  }, [findSibling, navigateToSet]);

  // Shift+Z: 確認なしで前のセットへ
  const goPrevSetParent = useCallback(async () => {
    const sibling = await findSibling("prev");
    if (sibling) navigateToSet(sibling.node_id);
  }, [findSibling, navigateToSet]);

  return { goNextSet, goPrevSet, goNextSetParent, goPrevSetParent, prompt, dismissPrompt };
}
