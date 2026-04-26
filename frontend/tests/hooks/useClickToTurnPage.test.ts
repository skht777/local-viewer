// useClickToTurnPage の振る舞い検証
// - 画面右半分クリック → handleNext
// - 画面左半分クリック → handlePrev
// - 中央 (clientX === mid) は左半分扱い

import { renderHook } from "@testing-library/react";
import { useClickToTurnPage } from "../../src/hooks/useClickToTurnPage";

function makeEvent(clientX: number, rect: { left: number; width: number }) {
  return {
    clientX,
    currentTarget: {
      getBoundingClientRect: () => ({
        left: rect.left,
        width: rect.width,
        right: rect.left + rect.width,
        top: 0,
        bottom: 0,
        x: rect.left,
        y: 0,
        height: 0,
        toJSON: () => ({}),
      }),
    },
  } as unknown as React.MouseEvent<HTMLDivElement>;
}

describe("useClickToTurnPage", () => {
  test("右半分クリックで handleNext が呼ばれる", () => {
    const handleNext = vi.fn();
    const handlePrev = vi.fn();
    const { result } = renderHook(() => useClickToTurnPage(handleNext, handlePrev));
    result.current(makeEvent(800, { left: 0, width: 1000 }));
    expect(handleNext).toHaveBeenCalledOnce();
    expect(handlePrev).not.toHaveBeenCalled();
  });

  test("左半分クリックで handlePrev が呼ばれる", () => {
    const handleNext = vi.fn();
    const handlePrev = vi.fn();
    const { result } = renderHook(() => useClickToTurnPage(handleNext, handlePrev));
    result.current(makeEvent(200, { left: 0, width: 1000 }));
    expect(handlePrev).toHaveBeenCalledOnce();
    expect(handleNext).not.toHaveBeenCalled();
  });

  test("中央 (clientX === mid) は左半分扱いで handlePrev", () => {
    const handleNext = vi.fn();
    const handlePrev = vi.fn();
    const { result } = renderHook(() => useClickToTurnPage(handleNext, handlePrev));
    // mid = 0 + 1000/2 = 500、clientX === mid のとき clientX > mid は false → prev
    result.current(makeEvent(500, { left: 0, width: 1000 }));
    expect(handlePrev).toHaveBeenCalledOnce();
    expect(handleNext).not.toHaveBeenCalled();
  });

  test("rect.left がオフセットを持つ場合も中点判定が機能する", () => {
    const handleNext = vi.fn();
    const handlePrev = vi.fn();
    const { result } = renderHook(() => useClickToTurnPage(handleNext, handlePrev));
    // mid = 100 + 200/2 = 200、clientX 250 > 200 → next
    result.current(makeEvent(250, { left: 100, width: 200 }));
    expect(handleNext).toHaveBeenCalledOnce();
  });
});
