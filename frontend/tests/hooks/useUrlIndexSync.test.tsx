// useUrlIndexSync の振る舞い検証
// - currentIndex === externalIndex のとき onChange を呼ばない
// - debounceMs=null のとき即時 onChange
// - debounceMs > 0 のとき setTimeout 経由で onChange
// - debounce 中に index 変更があると最後の値だけ反映される

import { renderHook } from "@testing-library/react";
import { useUrlIndexSync } from "../../src/hooks/useUrlIndexSync";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useUrlIndexSync", () => {
  test("currentIndex === externalIndex のとき onChange は呼ばれない", () => {
    const onChange = vi.fn();
    renderHook(() =>
      useUrlIndexSync({ currentIndex: 3, externalIndex: 3, onChange, debounceMs: null }),
    );
    expect(onChange).not.toHaveBeenCalled();
  });

  test("debounceMs=null のとき即時 onChange を呼ぶ", () => {
    const onChange = vi.fn();
    renderHook(() =>
      useUrlIndexSync({ currentIndex: 5, externalIndex: 0, onChange, debounceMs: null }),
    );
    expect(onChange).toHaveBeenCalledWith(5);
  });

  test("debounceMs > 0 のとき即時には呼ばず、タイマー経過後に呼ぶ", () => {
    const onChange = vi.fn();
    renderHook(() =>
      useUrlIndexSync({ currentIndex: 5, externalIndex: 0, onChange, debounceMs: 200 }),
    );
    expect(onChange).not.toHaveBeenCalled();
    vi.advanceTimersByTime(199);
    expect(onChange).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(onChange).toHaveBeenCalledWith(5);
  });

  test("debounce 中に currentIndex が変わると古いタイマーがキャンセルされる", () => {
    const onChange = vi.fn();
    const { rerender } = renderHook(
      ({ idx }: { idx: number }) =>
        useUrlIndexSync({
          currentIndex: idx,
          externalIndex: 0,
          onChange,
          debounceMs: 200,
        }),
      { initialProps: { idx: 1 } },
    );
    vi.advanceTimersByTime(100);
    rerender({ idx: 2 });
    vi.advanceTimersByTime(100);
    expect(onChange).not.toHaveBeenCalled();

    vi.advanceTimersByTime(100);
    expect(onChange).toHaveBeenCalledWith(2);
    expect(onChange).toHaveBeenCalledOnce();
  });

  test("unmount で pending タイマーがクリアされる", () => {
    const onChange = vi.fn();
    const { unmount } = renderHook(() =>
      useUrlIndexSync({ currentIndex: 5, externalIndex: 0, onChange, debounceMs: 200 }),
    );
    unmount();
    vi.advanceTimersByTime(500);
    expect(onChange).not.toHaveBeenCalled();
  });

  test("externalIndex に追従して onChange が抑止される", () => {
    const onChange = vi.fn();
    const { rerender } = renderHook(
      ({ ext }: { ext: number }) =>
        useUrlIndexSync({
          currentIndex: 5,
          externalIndex: ext,
          onChange,
          debounceMs: null,
        }),
      { initialProps: { ext: 0 } },
    );
    expect(onChange).toHaveBeenCalledWith(5);
    onChange.mockClear();

    // URL が同期して externalIndex も 5 になったら以後は呼ばれない
    rerender({ ext: 5 });
    expect(onChange).not.toHaveBeenCalled();
  });
});
