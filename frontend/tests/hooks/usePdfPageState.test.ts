// usePdfPageState の振る舞い検証
// - 内部 currentPage は 0-based、初期値 = initialPage - 1
// - handlePageChange(idx) で内部 state 更新 + onPageChange(idx + 1) 通知
// - setCurrentPage は onPageChange を発火しない

import { renderHook, act } from "@testing-library/react";
import { usePdfPageState } from "../../src/hooks/usePdfPageState";

describe("usePdfPageState", () => {
  test("initialPage=1 のとき currentPage=0", () => {
    const onPageChange = vi.fn();
    const { result } = renderHook(() => usePdfPageState(1, onPageChange));
    expect(result.current.currentPage).toBe(0);
    expect(onPageChange).not.toHaveBeenCalled();
  });

  test("initialPage=5 のとき currentPage=4", () => {
    const { result } = renderHook(() => usePdfPageState(5, vi.fn()));
    expect(result.current.currentPage).toBe(4);
  });

  test("handlePageChange は currentPage を更新し、1-based ページ番号で onPageChange を呼ぶ", () => {
    const onPageChange = vi.fn();
    const { result } = renderHook(() => usePdfPageState(1, onPageChange));
    act(() => result.current.handlePageChange(3));
    expect(result.current.currentPage).toBe(3);
    expect(onPageChange).toHaveBeenCalledWith(4);
  });

  test("setCurrentPage は内部 state のみ更新し onPageChange を発火しない", () => {
    const onPageChange = vi.fn();
    const { result } = renderHook(() => usePdfPageState(1, onPageChange));
    act(() => result.current.setCurrentPage(7));
    expect(result.current.currentPage).toBe(7);
    expect(onPageChange).not.toHaveBeenCalled();
  });

  test("複数回 handlePageChange を呼ぶと毎回 onPageChange が呼ばれる", () => {
    const onPageChange = vi.fn();
    const { result } = renderHook(() => usePdfPageState(1, onPageChange));
    act(() => result.current.handlePageChange(0));
    act(() => result.current.handlePageChange(2));
    expect(onPageChange).toHaveBeenCalledTimes(2);
    expect(onPageChange).toHaveBeenNthCalledWith(1, 1);
    expect(onPageChange).toHaveBeenNthCalledWith(2, 3);
  });
});
