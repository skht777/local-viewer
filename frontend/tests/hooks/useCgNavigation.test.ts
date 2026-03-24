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
