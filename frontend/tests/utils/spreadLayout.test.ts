// spreadLayout ユーティリティのテスト
// - ページグループの計算ロジックを検証

import { describe, test, expect } from "vitest";
import { computeSpreadGroup } from "../../src/utils/spreadLayout";

describe("computeSpreadGroup", () => {
  describe("single モード", () => {
    test("1ページを返す", () => {
      const result = computeSpreadGroup(3, 10, "single");
      expect(result.indices).toEqual([3]);
      expect(result.nextStart).toBe(4);
      expect(result.prevStart).toBe(2);
    });

    test("先頭ページで prevStart が null", () => {
      const result = computeSpreadGroup(0, 10, "single");
      expect(result.indices).toEqual([0]);
      expect(result.prevStart).toBeNull();
      expect(result.nextStart).toBe(1);
    });

    test("末尾ページで nextStart が null", () => {
      const result = computeSpreadGroup(9, 10, "single");
      expect(result.indices).toEqual([9]);
      expect(result.nextStart).toBeNull();
      expect(result.prevStart).toBe(8);
    });
  });

  describe("spread モード", () => {
    test("偶数始点ペアを返す", () => {
      const result = computeSpreadGroup(0, 10, "spread");
      expect(result.indices).toEqual([0, 1]);
      expect(result.nextStart).toBe(2);
      expect(result.prevStart).toBeNull();
    });

    test("奇数indexを偶数に正規化する", () => {
      const result = computeSpreadGroup(3, 10, "spread");
      expect(result.indices).toEqual([2, 3]);
      expect(result.nextStart).toBe(4);
      expect(result.prevStart).toBe(0);
    });

    test("最終ページが奇数の場合は単独表示", () => {
      // totalCount=9: [0,1] [2,3] [4,5] [6,7] [8]
      const result = computeSpreadGroup(8, 9, "spread");
      expect(result.indices).toEqual([8]);
      expect(result.nextStart).toBeNull();
      expect(result.prevStart).toBe(6);
    });

    test("中間ペアのナビゲーション", () => {
      // [4,5] in totalCount=10
      const result = computeSpreadGroup(4, 10, "spread");
      expect(result.indices).toEqual([4, 5]);
      expect(result.nextStart).toBe(6);
      expect(result.prevStart).toBe(2);
    });

    test("末尾ペアで nextStart が null", () => {
      // [8,9] in totalCount=10
      const result = computeSpreadGroup(8, 10, "spread");
      expect(result.indices).toEqual([8, 9]);
      expect(result.nextStart).toBeNull();
      expect(result.prevStart).toBe(6);
    });
  });

  describe("spread-offset モード", () => {
    test("index0は単独表示（表紙）", () => {
      const result = computeSpreadGroup(0, 10, "spread-offset");
      expect(result.indices).toEqual([0]);
      expect(result.nextStart).toBe(1);
      expect(result.prevStart).toBeNull();
    });

    test("index1以降はペアを返す", () => {
      // [1,2] in totalCount=10
      const result = computeSpreadGroup(1, 10, "spread-offset");
      expect(result.indices).toEqual([1, 2]);
      expect(result.nextStart).toBe(3);
      expect(result.prevStart).toBe(0);
    });

    test("偶数indexは奇数始点に正規化する", () => {
      // index=2 → [1,2]
      const result = computeSpreadGroup(2, 10, "spread-offset");
      expect(result.indices).toEqual([1, 2]);
    });

    test("中間ペア [3,4]", () => {
      const result = computeSpreadGroup(3, 10, "spread-offset");
      expect(result.indices).toEqual([3, 4]);
      expect(result.nextStart).toBe(5);
      expect(result.prevStart).toBe(1);
    });

    test("totalCount=2の場合 [0], [1]", () => {
      const r0 = computeSpreadGroup(0, 2, "spread-offset");
      expect(r0.indices).toEqual([0]);
      expect(r0.nextStart).toBe(1);

      const r1 = computeSpreadGroup(1, 2, "spread-offset");
      expect(r1.indices).toEqual([1]);
      expect(r1.nextStart).toBeNull();
      expect(r1.prevStart).toBe(0);
    });

    test("totalCount偶数で最終ページが単独", () => {
      // totalCount=10: [0], [1,2], [3,4], [5,6], [7,8], [9]
      const result = computeSpreadGroup(9, 10, "spread-offset");
      expect(result.indices).toEqual([9]);
      expect(result.nextStart).toBeNull();
      expect(result.prevStart).toBe(7);
    });
  });

  describe("エッジケース", () => {
    test("totalCount1の場合は全モードで単独表示", () => {
      for (const mode of ["single", "spread", "spread-offset"] as const) {
        const result = computeSpreadGroup(0, 1, mode);
        expect(result.indices).toEqual([0]);
        expect(result.nextStart).toBeNull();
        expect(result.prevStart).toBeNull();
      }
    });

    test("totalCount0の場合は空配列", () => {
      const result = computeSpreadGroup(0, 0, "single");
      expect(result.indices).toEqual([]);
      expect(result.nextStart).toBeNull();
      expect(result.prevStart).toBeNull();
    });
  });
});
