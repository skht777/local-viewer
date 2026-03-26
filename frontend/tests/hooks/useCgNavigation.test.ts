import { renderHook, act } from "@testing-library/react";
import { useCgNavigation } from "../../src/hooks/useCgNavigation";

describe("useCgNavigation", () => {
  const total = 5;

  test("goNext でインデックスが +1 される", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 2, setIndex));
    act(() => result.current.goNext());
    expect(setIndex).toHaveBeenCalledWith(3);
  });

  test("goPrev でインデックスが -1 される", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 2, setIndex));
    act(() => result.current.goPrev());
    expect(setIndex).toHaveBeenCalledWith(1);
  });

  test("最初の画像で goPrev しても setIndex が呼ばれない", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 0, setIndex));
    act(() => result.current.goPrev());
    expect(setIndex).not.toHaveBeenCalled();
  });

  test("最後の画像で goNext しても setIndex が呼ばれない", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 4, setIndex));
    act(() => result.current.goNext());
    expect(setIndex).not.toHaveBeenCalled();
  });

  test("goFirst で 0 になる", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 3, setIndex));
    act(() => result.current.goFirst());
    expect(setIndex).toHaveBeenCalledWith(0);
  });

  test("goLast で最後のインデックスになる", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 1, setIndex));
    act(() => result.current.goLast());
    expect(setIndex).toHaveBeenCalledWith(4);
  });

  test("goTo で指定インデックスにジャンプする", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 0, setIndex));
    act(() => result.current.goTo(3));
    expect(setIndex).toHaveBeenCalledWith(3);
  });

  test("canGoNext が最後のページで false", () => {
    const { result } = renderHook(() => useCgNavigation(total, 4, vi.fn()));
    expect(result.current.canGoNext).toBe(false);
  });

  test("canGoPrev が最初のページで false", () => {
    const { result } = renderHook(() => useCgNavigation(total, 0, vi.fn()));
    expect(result.current.canGoPrev).toBe(false);
  });

  test("goTo が範囲外の値を clamp する", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 0, setIndex));
    act(() => result.current.goTo(10));
    expect(setIndex).toHaveBeenCalledWith(4);
    act(() => result.current.goTo(-5));
    expect(setIndex).toHaveBeenCalledWith(0);
  });
});

describe("useCgNavigation — spread 対応", () => {
  const total = 10;

  test("spreadモードでdisplayIndicesが2要素を返す", () => {
    const { result } = renderHook(() => useCgNavigation(total, 0, vi.fn(), "spread"));
    expect(result.current.displayIndices).toEqual([0, 1]);
  });

  test("spreadモードでgoNextが2ページ分進む", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 0, setIndex, "spread"));
    act(() => result.current.goNext());
    expect(setIndex).toHaveBeenCalledWith(2);
  });

  test("spreadモードでgoPrevが2ページ分戻る", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 4, setIndex, "spread"));
    act(() => result.current.goPrev());
    expect(setIndex).toHaveBeenCalledWith(2);
  });

  test("spreadモードでgoToがグループ先頭に正規化される", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 0, setIndex, "spread"));
    act(() => result.current.goTo(3));
    // index=3 → グループ [2,3] の先頭 = 2
    expect(setIndex).toHaveBeenCalledWith(2);
  });

  test("spread-offsetモードでindex0は単独", () => {
    const { result } = renderHook(() => useCgNavigation(total, 0, vi.fn(), "spread-offset"));
    expect(result.current.displayIndices).toEqual([0]);
  });

  test("spread-offsetモードでindex1はペア", () => {
    const { result } = renderHook(() => useCgNavigation(total, 1, vi.fn(), "spread-offset"));
    expect(result.current.displayIndices).toEqual([1, 2]);
  });

  test("singleモードのデフォルト動作は変わらない", () => {
    const setIndex = vi.fn();
    const { result } = renderHook(() => useCgNavigation(total, 2, setIndex));
    expect(result.current.displayIndices).toEqual([2]);
    act(() => result.current.goNext());
    expect(setIndex).toHaveBeenCalledWith(3);
  });
});
