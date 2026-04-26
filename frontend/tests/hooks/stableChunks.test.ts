// 安定チャンク分割のテスト
// - 初回: 全チャンク構成
// - 追加のみ (無限スクロール): 既存チャンク維持 + 新規チャンク追加
// - 削除あり (タブ切替): 全チャンク再構成
// - 空配列入力: 空チャンクが返る

import { describe, expect, test } from "vitest";
import { computeStableChunks } from "../../src/hooks/api/thumbnailQueries";
import type { ChunkState } from "../../src/hooks/api/thumbnailQueries";

const EMPTY_STATE: ChunkState = { chunks: [], idSet: new Set() };

describe("computeStableChunks", () => {
  test("初回: 空の prev から全チャンク構成される", () => {
    const result = computeStableChunks(["a", "b", "c", "d", "e"], 3, EMPTY_STATE);
    expect(result.chunks).toEqual([
      ["a", "b", "c"],
      ["d", "e"],
    ]);
    expect(result.idSet).toEqual(new Set(["a", "b", "c", "d", "e"]));
  });

  test("追加のみ: 既存チャンクが維持され新規 ID のみ新チャンクになる", () => {
    const prev: ChunkState = {
      chunks: [
        ["a", "b", "c"],
        ["d", "e"],
      ],
      idSet: new Set(["a", "b", "c", "d", "e"]),
    };
    const result = computeStableChunks(["a", "b", "c", "d", "e", "f", "g"], 3, prev);
    // 既存チャンク [a,b,c], [d,e] はそのまま維持
    // 新規 ID [f,g] だけ新チャンクに
    expect(result.chunks).toEqual([
      ["a", "b", "c"],
      ["d", "e"],
      ["f", "g"],
    ]);
  });

  test("削除あり: 全チャンク再構成される", () => {
    const prev: ChunkState = {
      chunks: [
        ["a", "b", "c"],
        ["d", "e"],
      ],
      idSet: new Set(["a", "b", "c", "d", "e"]),
    };
    const result = computeStableChunks(["x", "y"], 3, prev);
    // 削除が検出されたので全再構成
    expect(result.chunks).toEqual([["x", "y"]]);
  });

  test("空配列入力: 空チャンクが返る", () => {
    const prev: ChunkState = {
      chunks: [["a", "b"]],
      idSet: new Set(["a", "b"]),
    };
    const result = computeStableChunks([], 3, prev);
    expect(result.chunks).toEqual([]);
    expect(result.idSet.size).toBe(0);
  });

  test("ID に変更がない場合は既存チャンクがそのまま返る", () => {
    const prev: ChunkState = {
      chunks: [["a", "b", "c"]],
      idSet: new Set(["a", "b", "c"]),
    };
    const result = computeStableChunks(["a", "b", "c"], 3, prev);
    // 参照が同じ (新チャンク追加なし)
    expect(result.chunks).toBe(prev.chunks);
  });

  test("順序が変わっても集合が同一なら既存チャンクが維持される", () => {
    const prev: ChunkState = {
      chunks: [["a", "b", "c"]],
      idSet: new Set(["a", "b", "c"]),
    };
    const result = computeStableChunks(["c", "a", "b"], 3, prev);
    expect(result.chunks).toBe(prev.chunks);
  });
});
