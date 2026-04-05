// sortEntries ユーティリティのテスト
// - name-asc/desc: ディレクトリ優先 + numeric localeCompare
// - date-asc/desc: null は末尾
// - 入力配列を破壊しない

import { describe, expect, test } from "vitest";
import type { BrowseEntry } from "../../src/types/api";
import { sortEntries } from "../../src/utils/sortEntries";

function entry(
  name: string,
  kind: BrowseEntry["kind"] = "image",
  modified_at: number | null = null,
): BrowseEntry {
  return {
    node_id: `id-${name}`,
    name,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at,
    preview_node_ids: null,
  };
}

describe("sortEntries", () => {
  describe("name-asc", () => {
    test("ディレクトリがファイルより先に来る", () => {
      const entries = [entry("b.jpg"), entry("a-dir", "directory"), entry("c.jpg")];
      const result = sortEntries(entries, "name-asc");
      expect(result[0].kind).toBe("directory");
    });

    test("同カテゴリ内で名前の自然順にソートされる", () => {
      const entries = [entry("file10.jpg"), entry("file2.jpg"), entry("file1.jpg")];
      const result = sortEntries(entries, "name-asc");
      expect(result.map((e) => e.name)).toEqual(["file1.jpg", "file2.jpg", "file10.jpg"]);
    });

    test("大文字小文字を区別しない", () => {
      const entries = [entry("Beta.jpg"), entry("alpha.jpg")];
      const result = sortEntries(entries, "name-asc");
      expect(result[0].name).toBe("alpha.jpg");
    });
  });

  describe("name-desc", () => {
    test("compareByName 全体が反転されるためディレクトリ優先は維持されない", () => {
      // -compareByName() なので directory 優先のスコアも反転される
      const entries = [entry("a-dir", "directory"), entry("z.jpg")];
      const result = sortEntries(entries, "name-desc");
      expect(result[0].name).toBe("z.jpg");
    });

    test("ファイルが名前の降順にソートされる", () => {
      const entries = [entry("a.jpg"), entry("c.jpg"), entry("b.jpg")];
      const result = sortEntries(entries, "name-desc");
      expect(result.map((e) => e.name)).toEqual(["c.jpg", "b.jpg", "a.jpg"]);
    });
  });

  describe("date-desc", () => {
    test("新しい順に並ぶ", () => {
      const entries = [entry("a", "image", 100), entry("b", "image", 300), entry("c", "image", 200)];
      const result = sortEntries(entries, "date-desc");
      expect(result.map((e) => e.modified_at)).toEqual([300, 200, 100]);
    });

    test("modified_at が null のエントリが末尾に来る", () => {
      const entries = [entry("a", "image", null), entry("b", "image", 100)];
      const result = sortEntries(entries, "date-desc");
      expect(result[0].modified_at).toBe(100);
      expect(result[1].modified_at).toBeNull();
    });
  });

  describe("date-asc", () => {
    test("古い順に並ぶ", () => {
      const entries = [entry("c", "image", 300), entry("a", "image", 100), entry("b", "image", 200)];
      const result = sortEntries(entries, "date-asc");
      expect(result.map((e) => e.modified_at)).toEqual([100, 200, 300]);
    });

    test("modified_at が null のエントリが末尾に来る", () => {
      const entries = [entry("a", "image", null), entry("b", "image", 50)];
      const result = sortEntries(entries, "date-asc");
      expect(result[0].modified_at).toBe(50);
      expect(result[1].modified_at).toBeNull();
    });

    test("両方 null の場合は順序が保持される", () => {
      const entries = [entry("first", "directory"), entry("second", "directory")];
      const result = sortEntries(entries, "date-asc");
      expect(result.map((e) => e.name)).toEqual(["first", "second"]);
    });
  });

  describe("共通", () => {
    test("入力配列を破壊しない", () => {
      const entries = [entry("b"), entry("a")];
      const original = [...entries];
      sortEntries(entries, "name-asc");
      expect(entries.map((e) => e.name)).toEqual(original.map((e) => e.name));
    });

    test("空配列で空配列を返す", () => {
      expect(sortEntries([], "name-asc")).toEqual([]);
    });
  });
});
