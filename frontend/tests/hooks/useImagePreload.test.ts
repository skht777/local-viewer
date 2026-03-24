import { renderHook } from "@testing-library/react";
import { useImagePreload } from "../../src/hooks/useImagePreload";
import type { BrowseEntry } from "../../src/types/api";

// プリロードされた URL を追跡
let preloadedSrcs: string[];
const OriginalImage = globalThis.Image;
beforeEach(() => {
  preloadedSrcs = [];
  // jsdom の Image は src 設定時に正規化するため、setter を上書きして追跡
  globalThis.Image = class MockImage {
    private _src = "";
    get src() {
      return this._src;
    }
    set src(value: string) {
      this._src = value;
      preloadedSrcs.push(value);
    }
  } as unknown as typeof Image;
});
afterEach(() => {
  globalThis.Image = OriginalImage;
});

function makeEntry(nodeId: string): BrowseEntry {
  return {
    node_id: nodeId,
    name: `${nodeId}.jpg`,
    kind: "image",
    size_bytes: 1024,
    mime_type: "image/jpeg",
    child_count: null,
  };
}

describe("useImagePreload", () => {
  test("現在インデックスの前後2枚をプリロードする", () => {
    const images = [makeEntry("a"), makeEntry("b"), makeEntry("c"), makeEntry("d"), makeEntry("e")];
    renderHook(() => useImagePreload(images, 2));
    const srcs = preloadedSrcs;
    // index=2 の前後2枚: index 0,1,3,4
    expect(srcs).toContain("/api/file/a");
    expect(srcs).toContain("/api/file/b");
    expect(srcs).toContain("/api/file/d");
    expect(srcs).toContain("/api/file/e");
  });

  test("インデックスが0のとき前方はプリロードしない", () => {
    const images = [makeEntry("a"), makeEntry("b"), makeEntry("c")];
    renderHook(() => useImagePreload(images, 0));
    const srcs = preloadedSrcs;
    // +1, +2 のみ
    expect(srcs).toContain("/api/file/b");
    expect(srcs).toContain("/api/file/c");
    expect(srcs).toHaveLength(2);
  });

  test("最後のインデックスのとき後方はプリロードしない", () => {
    const images = [makeEntry("a"), makeEntry("b"), makeEntry("c")];
    renderHook(() => useImagePreload(images, 2));
    const srcs = preloadedSrcs;
    // -1, -2 のみ
    expect(srcs).toContain("/api/file/a");
    expect(srcs).toContain("/api/file/b");
    expect(srcs).toHaveLength(2);
  });

  test("空の画像配列でエラーが発生しない", () => {
    renderHook(() => useImagePreload([], 0));
    expect(preloadedSrcs).toHaveLength(0);
  });
});
