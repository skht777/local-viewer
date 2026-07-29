// useRevealSelectedEntry の振る舞い検証
// - ?select= の対象が読み込み済みなら仮想スクロールで可視領域へ運ぶ
// - 未ロードなら hasMore の限り追加ページを取得して探し続ける
// - 見つからないまま打ち切られた場合は静かに終了する

import { renderHook } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { useRevealSelectedEntry } from "../../src/hooks/useRevealSelectedEntry";

function indexMapOf(ids: string[]): Map<string, number> {
  return new Map(ids.map((id, i) => [id, i]));
}

describe("useRevealSelectedEntry", () => {
  test("select 対象が読み込み済みなら該当 index へスクロールする", () => {
    const scrollToItem = vi.fn();
    renderHook(() =>
      useRevealSelectedEntry({
        selectedNodeId: "c",
        indexMap: indexMapOf(["a", "b", "c"]),
        scrollToItem,
      }),
    );
    expect(scrollToItem).toHaveBeenCalledWith(2);
  });

  test("selectedNodeId が無ければ何もしない", () => {
    const scrollToItem = vi.fn();
    const onLoadMore = vi.fn();
    renderHook(() =>
      useRevealSelectedEntry({
        indexMap: indexMapOf(["a"]),
        scrollToItem,
        hasMore: true,
        onLoadMore,
      }),
    );
    expect(scrollToItem).not.toHaveBeenCalled();
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("同じ selectedNodeId では再レンダリングしても 1 度しかスクロールしない", () => {
    const scrollToItem = vi.fn();
    const { rerender } = renderHook(
      ({ ids }: { ids: string[] }) =>
        useRevealSelectedEntry({
          selectedNodeId: "b",
          indexMap: indexMapOf(ids),
          scrollToItem,
        }),
      { initialProps: { ids: ["a", "b"] } },
    );
    rerender({ ids: ["a", "b"] });
    rerender({ ids: ["a", "b", "c"] });
    expect(scrollToItem).toHaveBeenCalledTimes(1);
  });

  test("selectedNodeId が変わると新しい対象へスクロールする", () => {
    const scrollToItem = vi.fn();
    const { rerender } = renderHook(
      ({ selected }: { selected: string }) =>
        useRevealSelectedEntry({
          selectedNodeId: selected,
          indexMap: indexMapOf(["a", "b", "c"]),
          scrollToItem,
        }),
      { initialProps: { selected: "a" } },
    );
    rerender({ selected: "c" });
    expect(scrollToItem).toHaveBeenNthCalledWith(1, 0);
    expect(scrollToItem).toHaveBeenNthCalledWith(2, 2);
  });

  test("select 対象が未ロードなら次ページを取得する", () => {
    const onLoadMore = vi.fn();
    const scrollToItem = vi.fn();
    renderHook(() =>
      useRevealSelectedEntry({
        selectedNodeId: "z",
        indexMap: indexMapOf(["a", "b"]),
        scrollToItem,
        hasMore: true,
        onLoadMore,
      }),
    );
    expect(onLoadMore).toHaveBeenCalledTimes(1);
    expect(scrollToItem).not.toHaveBeenCalled();
  });

  test("取得中は次ページ取得を重ねて発行しない", () => {
    const onLoadMore = vi.fn();
    renderHook(() =>
      useRevealSelectedEntry({
        selectedNodeId: "z",
        indexMap: indexMapOf(["a"]),
        scrollToItem: vi.fn(),
        hasMore: true,
        isLoadingMore: true,
        onLoadMore,
      }),
    );
    expect(onLoadMore).not.toHaveBeenCalled();
  });

  test("hasMore が false なら見つからなくても静かに終了する", () => {
    const onLoadMore = vi.fn();
    const scrollToItem = vi.fn();
    renderHook(() =>
      useRevealSelectedEntry({
        selectedNodeId: "z",
        indexMap: indexMapOf(["a", "b"]),
        scrollToItem,
        hasMore: false,
        onLoadMore,
      }),
    );
    expect(onLoadMore).not.toHaveBeenCalled();
    expect(scrollToItem).not.toHaveBeenCalled();
  });

  test("追加ページ到着で対象が見つかればスクロールする", () => {
    const onLoadMore = vi.fn();
    const scrollToItem = vi.fn();
    const { rerender } = renderHook(
      ({ ids, loading }: { ids: string[]; loading: boolean }) =>
        useRevealSelectedEntry({
          selectedNodeId: "z",
          indexMap: indexMapOf(ids),
          scrollToItem,
          hasMore: true,
          isLoadingMore: loading,
          onLoadMore,
        }),
      { initialProps: { ids: ["a", "b"], loading: false } },
    );
    expect(onLoadMore).toHaveBeenCalledTimes(1);

    rerender({ ids: ["a", "b", "z"], loading: false });
    expect(scrollToItem).toHaveBeenCalledWith(2);
    // 見つかった後は追加取得しない
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });
});
