// jumpListNavigation のユニットテスト
// - findInJumpList: 任意リスト範囲のセット間ジャンプ用の純粋関数（index ベース）
// - resolveJumpListAction: action 判定（fallback / boundary / navigate）
// - buildJumpListFromSearch: 検索結果から jumpList を構築
// - 境界（先頭/末尾）はループせず null を返す
// - index 範囲外 / null は boundary（FS フォールバックは list が null のときのみ）

import { describe, expect, test } from "vitest";
import {
  buildJumpListFromSearch,
  findInJumpList,
  resolveJumpListAction,
} from "../../src/lib/jumpListNavigation";
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

  test("next で次の entry と nextIndex を返す", () => {
    expect(findInJumpList("next", 0, list)).toEqual({ entry: list[1], nextIndex: 1 });
  });

  test("prev で前の entry と nextIndex を返す", () => {
    expect(findInJumpList("prev", 2, list)).toEqual({ entry: list[1], nextIndex: 1 });
  });

  test("末尾 index で next は境界扱い (null)", () => {
    expect(findInJumpList("next", 2, list)).toBeNull();
  });

  test("先頭 index で prev は境界扱い (null)", () => {
    expect(findInJumpList("prev", 0, list)).toBeNull();
  });

  test("currentIndex が null なら境界扱い (null)", () => {
    expect(findInJumpList("next", null, list)).toBeNull();
  });

  test("負の index は境界扱い (null)", () => {
    expect(findInJumpList("next", -1, list)).toBeNull();
  });

  test("リスト長以上の index は境界扱い (null)", () => {
    expect(findInJumpList("next", 3, list)).toBeNull();
  });

  test("空リストは常に境界扱い (null)", () => {
    expect(findInJumpList("next", 0, [])).toBeNull();
  });
});

describe("resolveJumpListAction", () => {
  const list: JumpListEntry[] = [makeJumpEntry("a"), makeJumpEntry("b"), makeJumpEntry("c")];

  test("list が null なら fallback", () => {
    expect(resolveJumpListAction("next", 0, null)).toEqual({ type: "fallback" });
  });

  test("currentIndex が null なら boundary", () => {
    expect(resolveJumpListAction("next", null, list)).toEqual({
      type: "boundary",
      message: "最後のセットです",
    });
  });

  test("末尾で next は boundary", () => {
    expect(resolveJumpListAction("next", 2, list)).toEqual({
      type: "boundary",
      message: "最後のセットです",
    });
  });

  test("先頭で prev は boundary", () => {
    expect(resolveJumpListAction("prev", 0, list)).toEqual({
      type: "boundary",
      message: "最初のセットです",
    });
  });

  test("通常遷移は navigate と nextIndex を返す", () => {
    expect(resolveJumpListAction("next", 0, list)).toEqual({
      type: "navigate",
      target: list[1],
      nextIndex: 1,
    });
  });

  test("PDF で parent_node_id が null なら boundary (セットを開けません)", () => {
    const orphan: JumpListEntry = {
      node_id: "pdf-orphan",
      parent_node_id: null,
      kind: "pdf",
      name: "pdf-orphan",
    };
    const listWithOrphan: JumpListEntry[] = [makeJumpEntry("a"), orphan];
    expect(resolveJumpListAction("next", 0, listWithOrphan)).toEqual({
      type: "boundary",
      message: "セットを開けません",
    });
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
