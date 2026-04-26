// useCursorAutoHide のユニットテスト
// - resetCursorTimer 呼び出しでカーソルが即時復活
// - 1 秒経過で cursor: none に切り替わる
// - 連続呼び出しで再カウント（debounce 動作）
// - unmount でタイマーがクリアされる

import { renderHook } from "@testing-library/react";
import { useRef } from "react";
import { useCursorAutoHide } from "../../src/hooks/useCursorAutoHide";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

function setupHook(timeoutMs?: number) {
  const target = document.createElement("div");
  document.body.append(target);
  const { result, unmount } = renderHook(() => {
    const ref = useRef<HTMLElement | null>(target);
    return useCursorAutoHide(ref, timeoutMs ? { timeoutMs } : undefined);
  });
  return {
    target,
    result,
    unmount: () => {
      unmount();
      target.remove();
    },
  };
}

describe("useCursorAutoHide", () => {
  test("resetCursorTimer 呼び出しでカーソルが即時に復活する", () => {
    const { target, result, unmount } = setupHook();
    target.style.cursor = "none";

    result.current.resetCursorTimer();
    expect(target.style.cursor).toBe("");

    unmount();
  });

  test("デフォルト 1 秒経過で cursor: none に切り替わる", () => {
    const { target, result, unmount } = setupHook();

    result.current.resetCursorTimer();
    expect(target.style.cursor).toBe("");

    vi.advanceTimersByTime(999);
    expect(target.style.cursor).toBe("");

    vi.advanceTimersByTime(1);
    expect(target.style.cursor).toBe("none");

    unmount();
  });

  test("連続呼び出し時はタイマーがリセットされる（debounce 動作）", () => {
    const { target, result, unmount } = setupHook();

    result.current.resetCursorTimer();
    vi.advanceTimersByTime(500);
    result.current.resetCursorTimer();
    vi.advanceTimersByTime(500);
    // 累計 1000ms だが 2 回目から 500ms しか経っていないので none にならない
    expect(target.style.cursor).toBe("");

    vi.advanceTimersByTime(500);
    expect(target.style.cursor).toBe("none");

    unmount();
  });

  test("timeoutMs オプションでタイムアウト値を指定できる", () => {
    const { target, result, unmount } = setupHook(500);

    result.current.resetCursorTimer();
    vi.advanceTimersByTime(499);
    expect(target.style.cursor).toBe("");

    vi.advanceTimersByTime(1);
    expect(target.style.cursor).toBe("none");

    unmount();
  });

  test("unmount でタイマーがクリアされ none にならない", () => {
    const { target, result, unmount } = setupHook();

    result.current.resetCursorTimer();
    unmount();
    target.style.cursor = "";
    vi.advanceTimersByTime(1000);
    expect(target.style.cursor).toBe("");
  });
});
