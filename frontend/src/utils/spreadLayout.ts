// 見開きレイアウトのページグループ計算
// - "single": 1ページ表示
// - "spread": 偶数始点ペア [0,1], [2,3], ...
// - "spread-offset": 表紙単独 [0], 以降ペア [1,2], [3,4], ...

import type { SpreadMode } from "../stores/viewerStore";

export interface SpreadGroup {
  indices: number[];
  nextStart: number | null;
  prevStart: number | null;
}

// ページグループを算出する
export function computeSpreadGroup(
  index: number,
  totalCount: number,
  spreadMode: SpreadMode,
): SpreadGroup {
  if (totalCount <= 0) {
    return { indices: [], nextStart: null, prevStart: null };
  }

  // index をクランプ
  const clamped = Math.max(0, Math.min(index, totalCount - 1));

  if (spreadMode === "single") {
    return {
      indices: [clamped],
      nextStart: clamped + 1 < totalCount ? clamped + 1 : null,
      prevStart: clamped > 0 ? clamped - 1 : null,
    };
  }

  if (spreadMode === "spread") {
    // 偶数始点に正規化
    const base = clamped - (clamped % 2);
    const indices = base + 1 < totalCount ? [base, base + 1] : [base];
    const lastIdx = indices[indices.length - 1];
    return {
      indices,
      nextStart: lastIdx + 1 < totalCount ? lastIdx + 1 : null,
      prevStart: base > 0 ? base - 2 : null,
    };
  }

  // spread-offset: [0], [1,2], [3,4], ...
  if (clamped === 0) {
    return {
      indices: [0],
      nextStart: 1 < totalCount ? 1 : null,
      prevStart: null,
    };
  }

  // clamped >= 1: 奇数始点に正規化
  // adjusted = clamped - 1 (0-based offset from index 1)
  const adjusted = clamped - 1;
  const base = 1 + adjusted - (adjusted % 2);
  const indices = base + 1 < totalCount ? [base, base + 1] : [base];
  const lastIdx = indices[indices.length - 1];
  return {
    indices,
    nextStart: lastIdx + 1 < totalCount ? lastIdx + 1 : null,
    prevStart: base <= 1 ? 0 : base - 2,
  };
}
