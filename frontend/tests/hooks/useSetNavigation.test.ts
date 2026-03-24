import { findNextSet, findPrevSet } from "../../src/hooks/useSetNavigation";
import type { BrowseEntry } from "../../src/types/api";

// セット間ジャンプのロジックは純粋関数としてテスト
// 実際の API 呼び出し（再帰降下）はフック内で行うが、
// 同階層内の候補探索ロジックは純粋関数で抽出

function entry(kind: BrowseEntry["kind"], id: string): BrowseEntry {
  return { node_id: id, name: id, kind, size_bytes: null, mime_type: null, child_count: null };
}

describe("findNextSet", () => {
  test("同ディレクトリ内の次のサブディレクトリを返す", () => {
    const siblings = [
      entry("image", "i1"),
      entry("directory", "d1"),
      entry("directory", "d2"),
    ];
    // 現在 d1 にいる場合、次は d2
    const result = findNextSet(siblings, "d1");
    expect(result?.node_id).toBe("d2");
  });

  test("現在のセットが最後の場合は null を返す", () => {
    const siblings = [
      entry("directory", "d1"),
      entry("directory", "d2"),
    ];
    const result = findNextSet(siblings, "d2");
    expect(result).toBeNull();
  });

  test("archive がセット候補に含まれる", () => {
    const siblings = [
      entry("directory", "d1"),
      entry("archive", "a1"),
      entry("pdf", "p1"),
      entry("directory", "d2"),
    ];
    // d1 の次はアーカイブ a1 (PDF はまだ Phase 6 なので候補外)
    const result = findNextSet(siblings, "d1");
    expect(result?.node_id).toBe("a1");
  });

  test("アーカイブの次のセットにディレクトリが返る", () => {
    const siblings = [
      entry("archive", "a1"),
      entry("directory", "d1"),
    ];
    const result = findNextSet(siblings, "a1");
    expect(result?.node_id).toBe("d1");
  });

  test("画像のみの場合は null（セット候補なし）", () => {
    const siblings = [
      entry("image", "i1"),
      entry("image", "i2"),
    ];
    const result = findNextSet(siblings, "i1");
    expect(result).toBeNull();
  });

  test("現在の nodeId が見つからない場合は最初のディレクトリを返す", () => {
    const siblings = [
      entry("directory", "d1"),
      entry("directory", "d2"),
    ];
    const result = findNextSet(siblings, "unknown");
    expect(result?.node_id).toBe("d1");
  });
});

describe("findPrevSet", () => {
  test("同ディレクトリ内の前のサブディレクトリを返す", () => {
    const siblings = [
      entry("directory", "d1"),
      entry("directory", "d2"),
      entry("image", "i1"),
    ];
    const result = findPrevSet(siblings, "d2");
    expect(result?.node_id).toBe("d1");
  });

  test("現在のセットが最初の場合は null を返す", () => {
    const siblings = [
      entry("directory", "d1"),
      entry("directory", "d2"),
    ];
    const result = findPrevSet(siblings, "d1");
    expect(result).toBeNull();
  });
});
