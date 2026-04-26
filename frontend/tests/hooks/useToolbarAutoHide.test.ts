import { renderHook, act } from "@testing-library/react";
import { useToolbarAutoHide } from "../../src/hooks/useToolbarAutoHide";

// matchMedia モック
const mockMatchMedia = (matches: boolean) => {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: query === "(pointer: coarse)" ? matches : false,
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  });
};

describe("useToolbarAutoHide", () => {
  beforeEach(() => {
    mockMatchMedia(false); // デフォルトはデスクトップ
  });

  test("デスクトップで初期状態は非表示", () => {
    const { result } = renderHook(() => useToolbarAutoHide());
    expect(result.current.isToolbarVisible).toBe(false);
    expect(result.current.isTouch).toBe(false);
  });

  test("タッチデバイスでは常に表示", () => {
    mockMatchMedia(true);
    const { result } = renderHook(() => useToolbarAutoHide());
    expect(result.current.isToolbarVisible).toBe(true);
    expect(result.current.isTouch).toBe(true);
  });

  test("マウスが上部付近に来ると表示される", () => {
    const container = document.createElement("div");
    container.getBoundingClientRect = () => ({
      top: 0,
      bottom: 800,
      left: 0,
      right: 1200,
      width: 1200,
      height: 800,
      x: 0,
      y: 0,
      toJSON: () => {},
    });
    const { result } = renderHook(() => useToolbarAutoHide());

    // コールバック ref でコンテナを登録
    act(() => {
      result.current.containerCallbackRef(container);
    });

    // pointermove を上部 (Y=30) で発火
    act(() => {
      container.dispatchEvent(new PointerEvent("pointermove", { clientY: 30, bubbles: true }));
    });
    expect(result.current.isToolbarVisible).toBe(true);
  });

  test("マウスが中央に移動すると非表示になる", () => {
    const container = document.createElement("div");
    container.getBoundingClientRect = () => ({
      top: 0,
      bottom: 800,
      left: 0,
      right: 1200,
      width: 1200,
      height: 800,
      x: 0,
      y: 0,
      toJSON: () => {},
    });
    const { result } = renderHook(() => useToolbarAutoHide());

    act(() => {
      result.current.containerCallbackRef(container);
    });

    // 上部に移動して表示
    act(() => {
      container.dispatchEvent(new PointerEvent("pointermove", { clientY: 30, bubbles: true }));
    });
    expect(result.current.isToolbarVisible).toBe(true);

    // 中央に移動して非表示
    act(() => {
      container.dispatchEvent(new PointerEvent("pointermove", { clientY: 400, bubbles: true }));
    });
    expect(result.current.isToolbarVisible).toBe(false);
  });

  test("pointerleave で非表示になる", () => {
    const container = document.createElement("div");
    container.getBoundingClientRect = () => ({
      top: 0,
      bottom: 800,
      left: 0,
      right: 1200,
      width: 1200,
      height: 800,
      x: 0,
      y: 0,
      toJSON: () => {},
    });
    const { result } = renderHook(() => useToolbarAutoHide());

    act(() => {
      result.current.containerCallbackRef(container);
    });

    act(() => {
      container.dispatchEvent(new PointerEvent("pointermove", { clientY: 30, bubbles: true }));
    });
    expect(result.current.isToolbarVisible).toBe(true);

    act(() => {
      container.dispatchEvent(new PointerEvent("pointerleave", { bubbles: true }));
    });
    expect(result.current.isToolbarVisible).toBe(false);
  });
});
