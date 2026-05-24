// 画像クリックでページ送り
// - 画面中央分割: 右半分 → 次、左半分 → 前
// - CgViewer / PdfCgViewer 共通
// - enabled=false で no-op (touch device で useTouchPageTurn と二重発火しないため)

import { useCallback } from "react";

export function useClickToTurnPage(
  handleNext: () => void,
  handlePrev: () => void,
  enabled: boolean = true,
): (e: React.MouseEvent<HTMLDivElement>) => void {
  return useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!enabled) {
        return;
      }
      const rect = e.currentTarget.getBoundingClientRect();
      const mid = rect.left + rect.width / 2;
      if (e.clientX > mid) {
        handleNext();
      } else {
        handlePrev();
      }
    },
    [handleNext, handlePrev, enabled],
  );
}
