// useViewerImages の振る舞い検証
// - images: ブラウズソート順を維持（image kind のみ）
// - viewerImages: 名前昇順固定
// - viewerIndexMap: ブラウズ順 index → ビューワー順 index
// - openViewerNameSorted: ブラウズ順 index を変換して openViewer 呼び出し

import { renderHook } from "@testing-library/react";
import { useViewerImages } from "../../src/hooks/useViewerImages";
import type { BrowseEntry } from "../../src/types/api";

function makeEntry(kind: BrowseEntry["kind"], id: string, name = id): BrowseEntry {
  return {
    node_id: id,
    name,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

describe("useViewerImages", () => {
  test("entries が undefined のときは images / viewerImages とも空配列", () => {
    const openViewer = vi.fn();
    const { result } = renderHook(() => useViewerImages(undefined, openViewer));
    expect(result.current.images).toEqual([]);
    expect(result.current.viewerImages).toEqual([]);
    expect(result.current.viewerIndexMap.size).toBe(0);
  });

  test("画像のみ抽出される (directory/video/pdf は除外)", () => {
    const entries = [
      makeEntry("directory", "d1"),
      makeEntry("image", "i1", "b.jpg"),
      makeEntry("video", "v1"),
      makeEntry("image", "i2", "a.jpg"),
      makeEntry("pdf", "p1"),
    ];
    const { result } = renderHook(() => useViewerImages(entries, vi.fn()));
    expect(result.current.images.map((e) => e.node_id)).toEqual(["i1", "i2"]);
  });

  test("viewerImages は name 昇順固定（images とブラウズ順が異なる）", () => {
    const entries = [makeEntry("image", "i1", "b.jpg"), makeEntry("image", "i2", "a.jpg")];
    const { result } = renderHook(() => useViewerImages(entries, vi.fn()));
    expect(result.current.images.map((e) => e.name)).toEqual(["b.jpg", "a.jpg"]);
    expect(result.current.viewerImages.map((e) => e.name)).toEqual(["a.jpg", "b.jpg"]);
  });

  test("viewerIndexMap が name 順 index を返す", () => {
    const entries = [makeEntry("image", "i1", "c.jpg"), makeEntry("image", "i2", "a.jpg")];
    const { result } = renderHook(() => useViewerImages(entries, vi.fn()));
    expect(result.current.viewerIndexMap.get("i2")).toBe(0);
    expect(result.current.viewerIndexMap.get("i1")).toBe(1);
  });

  test("openViewerNameSorted はブラウズ順 index をビューワー順に変換して openViewer を呼ぶ", () => {
    const openViewer = vi.fn();
    // ブラウズ順: c, a, b (image)
    const entries = [
      makeEntry("image", "ic", "c.jpg"),
      makeEntry("image", "ia", "a.jpg"),
      makeEntry("image", "ib", "b.jpg"),
    ];
    const { result } = renderHook(() => useViewerImages(entries, openViewer));
    // browseIndex=0 (c.jpg) → viewer 順 (a, b, c) では index=2
    result.current.openViewerNameSorted(0);
    expect(openViewer).toHaveBeenCalledWith(2);
  });

  test("範囲外の browseIndex は早期 return し openViewer を呼ばない", () => {
    const openViewer = vi.fn();
    const entries = [makeEntry("image", "i1")];
    const { result } = renderHook(() => useViewerImages(entries, openViewer));
    result.current.openViewerNameSorted(99);
    expect(openViewer).not.toHaveBeenCalled();
  });

  test("entries 変更時に images/viewerImages/Map が再計算される", () => {
    const openViewer = vi.fn();
    const initial = [makeEntry("image", "i1")];
    const { result, rerender } = renderHook(
      ({ entries }: { entries: BrowseEntry[] | undefined }) => useViewerImages(entries, openViewer),
      { initialProps: { entries: initial } },
    );
    expect(result.current.images).toHaveLength(1);

    rerender({
      entries: [makeEntry("image", "a"), makeEntry("image", "b")],
    });
    expect(result.current.images).toHaveLength(2);
    expect(result.current.viewerIndexMap.size).toBe(2);
  });
});
