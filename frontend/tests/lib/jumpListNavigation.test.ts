// jumpListNavigation のユニットテスト
// - findInJumpList: 任意リスト範囲のセット間ジャンプ用の純粋関数
// - buildJumpListFromSearch: 検索結果から jumpList を構築
// - 設計: リスト内に currentNodeId が無ければ null（FS フォールバックしない）
// - 境界（先頭/末尾）はループせず null を返す

import { describe, expect, test } from "vitest";
import { buildJumpListFromSearch, findInJumpList } from "../../src/lib/jumpListNavigation";
import type { JumpListEntry } from "../../src/lib/jumpListNavigation";
import type { SearchResult } from "../../src/types/api";

function makeJumpEntry(id: string, kind: JumpListEntry["kind"] = "directory"): JumpListEntry {
  return {
    node_id: id,
    parent_node_id: `parent-of-${id}`,
    kind,
    name: id,
  };
}

function makeSearchResult(
  id: string,
  kind: SearchResult["kind"],
  parent: string | null = `parent-of-${id}`,
): SearchResult {
  return {
    node_id: id,
    parent_node_id: parent,
    name: id,
    kind,
    relative_path: `path/${id}`,
    size_bytes: null,
  };
}

describe("findInJumpList", () => {
  const list: JumpListEntry[] = [makeJumpEntry("a"), makeJumpEntry("b"), makeJumpEntry("c")];

  test("next で次のエントリを返す", () => {
    expect(findInJumpList("next", "a", list)).toEqual(list[1]);
  });

  test("prev で前のエントリを返す", () => {
    expect(findInJumpList("prev", "c", list)).toEqual(list[1]);
  });

  test("末尾で next を押すと境界扱い (null)", () => {
    expect(findInJumpList("next", "c", list)).toBeNull();
  });

  test("先頭で prev を押すと境界扱い (null)", () => {
    expect(findInJumpList("prev", "a", list)).toBeNull();
  });

  test("リスト未登録の currentNodeId は境界扱い (null)", () => {
    expect(findInJumpList("next", "unknown", list)).toBeNull();
  });

  test("currentNodeId が null なら境界扱い (null)", () => {
    expect(findInJumpList("next", null, list)).toBeNull();
  });

  test("空リストは常に境界扱い (null)", () => {
    expect(findInJumpList("next", "a", [])).toBeNull();
  });
});

describe("buildJumpListFromSearch", () => {
  test("directory / archive / pdf のみを残す", () => {
    const entries: SearchResult[] = [
      makeSearchResult("dir1", "directory"),
      makeSearchResult("img1", "image"),
      makeSearchResult("vid1", "video"),
      makeSearchResult("arc1", "archive"),
      makeSearchResult("pdf1", "pdf"),
      makeSearchResult("oth1", "other"),
    ];
    const list = buildJumpListFromSearch(entries);
    expect(list.map((e) => e.node_id)).toEqual(["dir1", "arc1", "pdf1"]);
  });

  test("入力順序を保つ", () => {
    const entries: SearchResult[] = [
      makeSearchResult("pdf1", "pdf"),
      makeSearchResult("arc1", "archive"),
      makeSearchResult("dir1", "directory"),
    ];
    const list = buildJumpListFromSearch(entries);
    expect(list.map((e) => e.node_id)).toEqual(["pdf1", "arc1", "dir1"]);
  });

  test("parent_node_id が null でもそのまま保持される", () => {
    const entries: SearchResult[] = [makeSearchResult("pdf-orphan", "pdf", null)];
    const list = buildJumpListFromSearch(entries);
    expect(list[0].parent_node_id).toBeNull();
  });

  test("最小フィールドのみマッピングする", () => {
    const entries: SearchResult[] = [makeSearchResult("dir1", "directory")];
    const list = buildJumpListFromSearch(entries);
    expect(list[0]).toEqual({
      node_id: "dir1",
      parent_node_id: "parent-of-dir1",
      kind: "directory",
      name: "dir1",
    });
  });
});
