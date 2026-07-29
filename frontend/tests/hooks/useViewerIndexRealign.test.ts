// useViewerIndexRealign の振る舞い検証
// - 画像リスト再構築 (全ページ後追い到着等) で表示中画像の位置がずれたら index を補正
// - ユーザーのページ送り (index 変更のみ) では補正しない

import { renderHook } from "@testing-library/react";
import { useViewerIndexRealign } from "../../src/hooks/useViewerIndexRealign";
import type { BrowseEntry } from "../../src/types/api";

function makeImage(id: string): BrowseEntry {
  return {
    node_id: id,
    name: `${id}.jpg`,
    kind: "image",
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

interface Props {
  isViewerOpen: boolean;
  viewerImages: BrowseEntry[];
  index: number;
}

function run(initial: Props) {
  const setIndex = vi.fn();
  const view = renderHook((p: Props) => useViewerIndexRealign({ ...p, setIndex }), {
    initialProps: initial,
  });
  return { setIndex, ...view };
}

const b = makeImage("b");
const c = makeImage("c");
const a = makeImage("a");

describe("useViewerIndexRealign", () => {
  test("画像リスト再構築で表示中画像の新しい index へ補正する", () => {
    // index=0 で b を表示中 → 後追いページで a が名前順先頭に挿入される
    const imgs1 = [b, c];
    const { setIndex, rerender } = run({ isViewerOpen: true, viewerImages: imgs1, index: 0 });
    expect(setIndex).not.toHaveBeenCalled();

    const imgs2 = [a, b, c];
    rerender({ isViewerOpen: true, viewerImages: imgs2, index: 0 });
    expect(setIndex).toHaveBeenCalledWith(1);
  });

  test("ユーザーのページ送り (index 変更のみ) では補正しない", () => {
    const imgs = [a, b, c];
    const { setIndex, rerender } = run({ isViewerOpen: true, viewerImages: imgs, index: 0 });
    rerender({ isViewerOpen: true, viewerImages: imgs, index: 1 });
    rerender({ isViewerOpen: true, viewerImages: imgs, index: 2 });
    expect(setIndex).not.toHaveBeenCalled();
  });

  test("リスト再構築でも位置が変わらなければ補正しない", () => {
    // 末尾への追加は表示中画像の位置に影響しない
    const imgs1 = [a, b];
    const { setIndex, rerender } = run({ isViewerOpen: true, viewerImages: imgs1, index: 1 });
    const imgs2 = [a, b, c];
    rerender({ isViewerOpen: true, viewerImages: imgs2, index: 1 });
    expect(setIndex).not.toHaveBeenCalled();
  });

  test("表示中画像がリストから消えた場合は補正しない (clamp に任せる)", () => {
    const imgs1 = [a, b];
    const { setIndex, rerender } = run({ isViewerOpen: true, viewerImages: imgs1, index: 1 });
    const imgs2 = [a, c];
    rerender({ isViewerOpen: true, viewerImages: imgs2, index: 1 });
    expect(setIndex).not.toHaveBeenCalled();
  });

  test("ビューワー非表示中は何もしない", () => {
    const imgs1 = [b, c];
    const { setIndex, rerender } = run({ isViewerOpen: false, viewerImages: imgs1, index: 0 });
    rerender({ isViewerOpen: false, viewerImages: [a, b, c], index: 0 });
    expect(setIndex).not.toHaveBeenCalled();
  });
});
