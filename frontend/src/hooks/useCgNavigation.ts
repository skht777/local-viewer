// CGモードのページ内ナビゲーション
// - 画像配列内のインデックス操作（前後/先頭/末尾/直接ジャンプ）
// - spreadMode に応じたグループ単位のナビゲーション
// - セット間移動は useSetNavigation が担当

import { useCallback, useMemo } from "react";
import type { SpreadMode } from "../stores/viewerStore";
import { computeSpreadGroup } from "../utils/spreadLayout";

interface UseCgNavigationReturn {
  goNext: () => void;
  goPrev: () => void;
  goFirst: () => void;
  goLast: () => void;
  goTo: (index: number) => void;
  canGoNext: boolean;
  canGoPrev: boolean;
  displayIndices: number[];
}

export function useCgNavigation(
  totalCount: number,
  currentIndex: number,
  setIndex: (index: number) => void,
  spreadMode: SpreadMode = "single",
): UseCgNavigationReturn {
  const group = useMemo(
    () => computeSpreadGroup(currentIndex, totalCount, spreadMode),
    [currentIndex, totalCount, spreadMode],
  );

  const canGoNext = group.nextStart !== null;
  const canGoPrev = group.prevStart !== null;

  const goNext = useCallback(() => {
    if (group.nextStart !== null) {
      setIndex(group.nextStart);
    }
  }, [group.nextStart, setIndex]);

  const goPrev = useCallback(() => {
    if (group.prevStart !== null) {
      setIndex(group.prevStart);
    }
  }, [group.prevStart, setIndex]);

  const goFirst = useCallback(() => {
    setIndex(0);
  }, [setIndex]);

  const goLast = useCallback(() => {
    // 最終グループの先頭に移動
    const lastGroup = computeSpreadGroup(totalCount - 1, totalCount, spreadMode);
    setIndex(lastGroup.indices[0] ?? totalCount - 1);
  }, [setIndex, totalCount, spreadMode]);

  const goTo = useCallback(
    (index: number) => {
      const clamped = Math.max(0, Math.min(totalCount - 1, index));
      // グループ先頭に正規化
      const targetGroup = computeSpreadGroup(clamped, totalCount, spreadMode);
      setIndex(targetGroup.indices[0] ?? clamped);
    },
    [setIndex, totalCount, spreadMode],
  );

  return useMemo(
    () => ({
      goNext,
      goPrev,
      goFirst,
      goLast,
      goTo,
      canGoNext,
      canGoPrev,
      displayIndices: group.indices,
    }),
    [goNext, goPrev, goFirst, goLast, goTo, canGoNext, canGoPrev, group.indices],
  );
}
