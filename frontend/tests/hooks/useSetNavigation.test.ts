import {
  findNextSet,
  findPrevSet,
  resolveTopLevelDir,
  shouldConfirm,
} from "../../src/hooks/useSetNavigation";
import type { AncestorEntry, BrowseEntry } from "../../src/types/api";

// セット間ジャンプのロジックは純粋関数としてテスト
// 実際の API 呼び出し（再帰降下）はフック内で行うが、
// 同階層内の候補探索ロジックは純粋関数で抽出

function entry(kind: BrowseEntry["kind"], id: string): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

describe("findNextSet", () => {
  test("同ディレクトリ内の次のサブディレクトリを返す", () => {
    const siblings = [entry("image", "i1"), entry("directory", "d1"), entry("directory", "d2")];
    // 現在 d1 にいる場合、次は d2
    const result = findNextSet(siblings, "d1");
    expect(result?.node_id).toBe("d2");
  });

  test("現在のセットが最後の場合は null を返す", () => {
    const siblings = [entry("directory", "d1"), entry("directory", "d2")];
    const result = findNextSet(siblings, "d2");
    expect(result).toBeNull();
  });

  test("archive がセット候補に含まれる", () => {
    const siblings = [entry("directory", "d1"), entry("archive", "a1"), entry("directory", "d2")];
    const result = findNextSet(siblings, "d1");
    expect(result?.node_id).toBe("a1");
  });

  test("PDF がセット候補に含まれる", () => {
    const siblings = [entry("directory", "d1"), entry("pdf", "p1"), entry("directory", "d2")];
    const result = findNextSet(siblings, "d1");
    expect(result?.node_id).toBe("p1");
  });

  test("PDF→次のセット候補を返す", () => {
    const siblings = [entry("pdf", "p1"), entry("archive", "a1"), entry("directory", "d1")];
    const result = findNextSet(siblings, "p1");
    expect(result?.node_id).toBe("a1");
  });

  test("アーカイブの次のセットにディレクトリが返る", () => {
    const siblings = [entry("archive", "a1"), entry("directory", "d1")];
    const result = findNextSet(siblings, "a1");
    expect(result?.node_id).toBe("d1");
  });

  test("画像のみの場合は null（セット候補なし）", () => {
    const siblings = [entry("image", "i1"), entry("image", "i2")];
    const result = findNextSet(siblings, "i1");
    expect(result).toBeNull();
  });

  test("現在の nodeId が見つからない場合は最初のディレクトリを返す", () => {
    const siblings = [entry("directory", "d1"), entry("directory", "d2")];
    const result = findNextSet(siblings, "unknown");
    expect(result?.node_id).toBe("d1");
  });
});

function ancestor(id: string, name: string): AncestorEntry {
  return { node_id: id, name };
}

describe("resolveTopLevelDir", () => {
  test("ancestors が2要素以上なら ancestors[1] を返す", () => {
    const ancestors = [
      ancestor("root", "Root"),
      ancestor("topA", "TopDirA"),
      ancestor("sub", "SubDir"),
    ];
    const result = resolveTopLevelDir(ancestors, "sub", entry("archive", "a1"));
    expect(result).toBe("topA");
  });

  test("ancestors が1要素なら探索ディレクトリ自体を返す", () => {
    const ancestors = [ancestor("root", "Root")];
    const result = resolveTopLevelDir(ancestors, "topA", entry("archive", "a1"));
    expect(result).toBe("topA");
  });

  test("ancestors が空でエントリがディレクトリなら自身を返す", () => {
    const result = resolveTopLevelDir([], "root", entry("directory", "topB"));
    expect(result).toBe("topB");
  });

  test("ancestors が空でエントリがアーカイブなら null を返す", () => {
    const result = resolveTopLevelDir([], "root", entry("archive", "rootFile"));
    expect(result).toBeNull();
  });

  test("ancestors が空でエントリが PDF なら null を返す", () => {
    const result = resolveTopLevelDir([], "root", entry("pdf", "rootPdf"));
    expect(result).toBeNull();
  });
});

describe("shouldConfirm", () => {
  test("levelsUp が 2 以上なら確認あり", () => {
    expect(shouldConfirm(2, "topA", "topB")).toBe(true);
  });

  test("levelsUp が 2 以上なら topDir が同じでも確認あり", () => {
    expect(shouldConfirm(2, "topA", "topA")).toBe(true);
  });

  test("topDir が異なれば levelsUp が 0 でも確認あり", () => {
    expect(shouldConfirm(0, "topA", "topB")).toBe(true);
  });

  test("topDir が null から non-null に変わると確認あり", () => {
    expect(shouldConfirm(0, null, "topC")).toBe(true);
  });

  test("topDir が non-null から null に変わると確認あり", () => {
    expect(shouldConfirm(1, "topC", null)).toBe(true);
  });

  test("topDir が同じなら確認なし", () => {
    expect(shouldConfirm(0, "topA", "topA")).toBe(false);
    expect(shouldConfirm(1, "topA", "topA")).toBe(false);
  });

  test("両方 null (ルート直下ファイル間) なら確認なし", () => {
    expect(shouldConfirm(0, null, null)).toBe(false);
  });
});

describe("findPrevSet", () => {
  test("同ディレクトリ内の前のサブディレクトリを返す", () => {
    const siblings = [entry("directory", "d1"), entry("directory", "d2"), entry("image", "i1")];
    const result = findPrevSet(siblings, "d2");
    expect(result?.node_id).toBe("d1");
  });

  test("現在のセットが最初の場合は null を返す", () => {
    const siblings = [entry("directory", "d1"), entry("directory", "d2")];
    const result = findPrevSet(siblings, "d1");
    expect(result).toBeNull();
  });
});
