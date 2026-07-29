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

  test("showToast に duration override を渡すと override 後の時間で消える", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("タイトル", 3000);
    });
    expect(result.current.toastMessage).toBe("タイトル");
    // 2000ms 経過時点ではまだ表示維持
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(result.current.toastMessage).toBe("タイトル");
    // 残り 1000ms で消える
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current.toastMessage).toBeNull();
  });

  test("showToast に duration を省略するとフックの duration で消える", () => {
    const { result } = renderHook(() => useToast(500));
    act(() => {
      result.current.showToast("ショート");
    });
    expect(result.current.toastMessage).toBe("ショート");
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(result.current.toastMessage).toBeNull();
  });

  // V13 回帰: 同一メッセージの再表示でも旧タイマーを破棄し、表示時間をフルにリセットする
  test("同一メッセージを再表示すると表示時間がフルにリセットされる", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("同じメッセージ");
    });
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    // 同じ文言で再表示（state が変わらないため旧タイマーの取りこぼしが起きやすい）
    act(() => {
      result.current.showToast("同じメッセージ");
    });
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    // 旧タイマーが残っていれば 1500ms 時点で消えてしまう
    expect(result.current.toastMessage).toBe("同じメッセージ");
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(result.current.toastMessage).toBeNull();
  });

  // V13 回帰: アンマウント後にタイマーが生き残らないこと
  test("アンマウント時に自動消去タイマーが破棄される", () => {
    const { result, unmount } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("アンマウント前");
    });
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  // duration override 後にデフォルト duration へ戻ることを消去タイミングで確認する
  // (toastDuration は Toast 側の二重タイマー撤去に伴い廃止)
  test("duration override の次の showToast はフックのデフォルト duration に戻る", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.showToast("ロング", 3000);
    });
    act(() => {
      result.current.showToast("デフォルトに戻る");
    });
    act(() => {
      vi.advanceTimersByTime(1999);
    });
    expect(result.current.toastMessage).toBe("デフォルトに戻る");
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current.toastMessage).toBeNull();
  });
});
