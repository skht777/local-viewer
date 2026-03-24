// CGモードのページ内ナビゲーション
// - 画像配列内のインデックス操作（前後/先頭/末尾/直接ジャンプ）
// - セット間移動は useSetNavigation が担当

import { useCallback, useMemo } from "react";

interface UseCgNavigationReturn {
  goNext: () => void;
  goPrev: () => void;
  goFirst: () => void;
  goLast: () => void;
  goTo: (index: number) => void;
  canGoNext: boolean;
  canGoPrev: boolean;
}

export function useCgNavigation(
  totalCount: number,
  currentIndex: number,
  setIndex: (index: number) => void,
): UseCgNavigationReturn {
  const canGoNext = currentIndex < totalCount - 1;
  const canGoPrev = currentIndex > 0;

  const goNext = useCallback(() => {
    if (canGoNext) setIndex(currentIndex + 1);
  }, [canGoNext, currentIndex, setIndex]);

  const goPrev = useCallback(() => {
    if (canGoPrev) setIndex(currentIndex - 1);
  }, [canGoPrev, currentIndex, setIndex]);

  const goFirst = useCallback(() => {
    setIndex(0);
  }, [setIndex]);

  const goLast = useCallback(() => {
    setIndex(totalCount - 1);
  }, [setIndex, totalCount]);

  const goTo = useCallback(
    (index: number) => {
      setIndex(Math.max(0, Math.min(totalCount - 1, index)));
    },
    [setIndex, totalCount],
  );

  return useMemo(
    () => ({ goNext, goPrev, goFirst, goLast, goTo, canGoNext, canGoPrev }),
    [goNext, goPrev, goFirst, goLast, goTo, canGoNext, canGoPrev],
  );
}
