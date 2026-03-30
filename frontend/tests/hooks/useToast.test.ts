import { renderHook, act } from "@testing-library/react";
import { useToast } from "../../src/hooks/useToast";

describe("useToast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  test("初期状態で toastMessage が null", () => {
    const { result } = renderHook(() => useToast());
    expect(result.current.toastMessage).toBeNull();
  });

  test("showToast でメッセージが設定される", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("最後の画像です");
    });
    expect(result.current.toastMessage).toBe("最後の画像です");
  });

  test("2秒後に自動で toastMessage が null になる", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("テスト");
    });
    expect(result.current.toastMessage).toBe("テスト");
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(result.current.toastMessage).toBeNull();
  });

  test("dismissToast で即座に消去できる", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("テスト");
    });
    act(() => {
      result.current.dismissToast();
    });
    expect(result.current.toastMessage).toBeNull();
  });

  test("連続呼び出しでタイマーがリセットされる", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("1回目");
    });
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    act(() => {
      result.current.showToast("2回目");
    });
    expect(result.current.toastMessage).toBe("2回目");
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    // 2回目から2秒経っていないのでまだ表示
    expect(result.current.toastMessage).toBe("2回目");
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(result.current.toastMessage).toBeNull();
  });
});
