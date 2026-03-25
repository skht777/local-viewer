// セット間ジャンプの実行オーケストレーション
// - findNextSet/findPrevSet（純粋関数）で同階層の次/前を探索
// - 候補がなければ親ディレクトリの browse API を呼んで兄弟を走査
// - NavigationPrompt の状態管理を内包
// - CgViewer / MangaViewer / PdfCgViewer / PdfMangaViewer から共通利用
// - PDF の場合は ?pdf= 付き URL で遷移 (browse 422 回避)

import { useCallback, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { browseNodeOptions } from "./api/browseQueries";
import { findNextSet, findPrevSet } from "./useSetNavigation";
import type { ViewerMode } from "./useViewerParams";
import type { BrowseEntry } from "../types/api";

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

  // 遷移先の kind に応じた URL で遷移
  // - PDF: 親ディレクトリに留まり ?pdf= 付きで PDF ビューワーを開く
  // - ディレクトリ/アーカイブ: 従来通り browse
  const navigateToTarget = useCallback(
    (target: BrowseEntry) => {
      if (target.kind === "pdf") {
        navigate(`/browse/${parentNodeId}?pdf=${target.node_id}&page=1&mode=${mode}`);
      } else {
        navigate(`/browse/${target.node_id}?tab=images&index=0&mode=${mode}`);
      }
    },
    [navigate, parentNodeId, mode],
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
        navigateToTarget(sibling);
      },
      onCancel: () => setPrompt(null),
    });
  }, [findSibling, navigateToTarget]);

  // PageUp/Z: 確認ダイアログ付きで前のセットへ
  const goPrevSet = useCallback(async () => {
    const sibling = await findSibling("prev");
    if (!sibling) return;
    setPrompt({
      message: "前のディレクトリに移動しますか？",
      onConfirm: () => {
        setPrompt(null);
        navigateToTarget(sibling);
      },
      onCancel: () => setPrompt(null),
    });
  }, [findSibling, navigateToTarget]);

  // Shift+X: 確認なしで次のセットへ
  const goNextSetParent = useCallback(async () => {
    const sibling = await findSibling("next");
    if (sibling) navigateToTarget(sibling);
  }, [findSibling, navigateToTarget]);

  // Shift+Z: 確認なしで前のセットへ
  const goPrevSetParent = useCallback(async () => {
    const sibling = await findSibling("prev");
    if (sibling) navigateToTarget(sibling);
  }, [findSibling, navigateToTarget]);

  return { goNextSet, goPrevSet, goNextSetParent, goPrevSetParent, prompt, dismissPrompt };
}
