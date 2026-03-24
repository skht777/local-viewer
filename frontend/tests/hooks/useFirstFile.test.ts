import { selectFirstViewable } from "../../src/hooks/useFirstFile";
import type { BrowseEntry } from "../../src/types/api";

function entry(kind: BrowseEntry["kind"], id: string): BrowseEntry {
  return { node_id: id, name: id, kind, size_bytes: null, mime_type: null, child_count: null };
}

describe("selectFirstViewable", () => {
  test("画像がある場合は最初の画像を返す", () => {
    const entries = [entry("directory", "d1"), entry("image", "i1"), entry("image", "i2")];
    expect(selectFirstViewable(entries)?.node_id).toBe("i1");
  });

  test("画像がなくディレクトリのみの場合はディレクトリを返す", () => {
    const entries = [entry("directory", "d1"), entry("directory", "d2")];
    expect(selectFirstViewable(entries)?.node_id).toBe("d1");
  });

  test("archive が最優先で選択される", () => {
    const entries = [entry("image", "i1"), entry("archive", "a1"), entry("directory", "d1")];
    expect(selectFirstViewable(entries)?.node_id).toBe("a1");
  });

  test("archive > image > directory の優先順位", () => {
    const entries = [entry("directory", "d1"), entry("image", "i1")];
    expect(selectFirstViewable(entries)?.node_id).toBe("i1");

    const entries2 = [entry("directory", "d1"), entry("archive", "a1")];
    expect(selectFirstViewable(entries2)?.node_id).toBe("a1");
  });

  test("空の配列で null を返す", () => {
    expect(selectFirstViewable([])).toBeNull();
  });

  test("PDF のみの場合は null を返す (Phase 6 で対応)", () => {
    const entries = [entry("pdf", "p1")];
    expect(selectFirstViewable(entries)).toBeNull();
  });

  test("video/other はスキップされる", () => {
    const entries = [entry("video", "v1"), entry("other", "o1"), entry("image", "i1")];
    expect(selectFirstViewable(entries)?.node_id).toBe("i1");
  });
});
