// searchResultToBrowseEntry のテスト
// - SearchResult の必須フィールドが BrowseEntry に正しく反映される
// - 拡張フィールド（modified_at, mime_type, child_count, preview_node_ids）が
//   undefined の場合に null フォールバックされる

import { describe, expect, test } from "vitest";
import { searchResultToBrowseEntry } from "../../src/utils/searchResultToBrowseEntry";
import type { SearchResult } from "../../src/types/api";

function baseResult(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    node_id: "n1",
    parent_node_id: null,
    name: "test.jpg",
    kind: "image",
    relative_path: "photos/test.jpg",
    size_bytes: 1024,
    ...overrides,
  };
}

describe("searchResultToBrowseEntry", () => {
  test("基本フィールドが BrowseEntry に反映される", () => {
    const entry = searchResultToBrowseEntry(baseResult());
    expect(entry.node_id).toBe("n1");
    expect(entry.name).toBe("test.jpg");
    expect(entry.kind).toBe("image");
    expect(entry.size_bytes).toBe(1024);
  });

  test("拡張フィールドが undefined のとき null になる", () => {
    const entry = searchResultToBrowseEntry(baseResult());
    expect(entry.modified_at).toBeNull();
    expect(entry.mime_type).toBeNull();
    expect(entry.child_count).toBeNull();
    expect(entry.preview_node_ids).toBeNull();
  });

  test("拡張フィールドが指定されていれば反映される", () => {
    const entry = searchResultToBrowseEntry(
      baseResult({
        modified_at: 1700000000,
        mime_type: "image/jpeg",
        child_count: 5,
        preview_node_ids: ["a", "b"],
      }),
    );
    expect(entry.modified_at).toBe(1700000000);
    expect(entry.mime_type).toBe("image/jpeg");
    expect(entry.child_count).toBe(5);
    expect(entry.preview_node_ids).toEqual(["a", "b"]);
  });

  test("kind=other はそのまま other になる", () => {
    const entry = searchResultToBrowseEntry(baseResult({ kind: "other" }));
    expect(entry.kind).toBe("other");
  });
});
