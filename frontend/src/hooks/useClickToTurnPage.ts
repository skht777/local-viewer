// 画像クリックでページ送り
// - 画面中央分割: 右半分 → 次、左半分 → 前
// - CgViewer / PdfCgViewer 共通

import { useCallback } from "react";

export function useClickToTurnPage(
  handleNext: () => void,
  handlePrev: () => void,
): (e: React.MouseEvent<HTMLDivElement>) => void {
  return useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const mid = rect.left + rect.width / 2;
      if (e.clientX > mid) {
        handleNext();
      } else {
        handlePrev();
      }
    },
    [handleNext, handlePrev],
  );
}
