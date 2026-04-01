import { renderHook } from "@testing-library/react";
import { useTreeKeyboard } from "../../src/hooks/useTreeKeyboard";

const KEY_CODE_MAP: Record<string, string> = {
  ArrowRight: "ArrowRight",
  ArrowLeft: "ArrowLeft",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  Enter: "Enter",
  Home: "Home",
  End: "End",
  t: "KeyT",
};

function pressKey(key: string, options: KeyboardEventInit = {}) {
  const code = KEY_CODE_MAP[key] ?? key;
  const init: KeyboardEventInit = { key, code, bubbles: true, cancelable: true, ...options };
  document.dispatchEvent(new KeyboardEvent("keydown", init));
  document.dispatchEvent(new KeyboardEvent("keyup", init));
}

describe("useTreeKeyboard", () => {
  const defaultCallbacks = {
    moveUp: vi.fn(),
    moveDown: vi.fn(),
    expand: vi.fn(),
    collapse: vi.fn(),
    select: vi.fn(),
    goFirst: vi.fn(),
    goLast: vi.fn(),
    focusBrowser: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("↓ キーで moveDown が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("ArrowDown");
    expect(defaultCallbacks.moveDown).toHaveBeenCalledOnce();
  });

  test("↑ キーで moveUp が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("ArrowUp");
    expect(defaultCallbacks.moveUp).toHaveBeenCalledOnce();
  });

  test("→ キーで expand が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("ArrowRight");
    expect(defaultCallbacks.expand).toHaveBeenCalledOnce();
  });

  test("← キーで collapse が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("ArrowLeft");
    expect(defaultCallbacks.collapse).toHaveBeenCalledOnce();
  });

  test("Enter キーで select が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("Enter");
    expect(defaultCallbacks.select).toHaveBeenCalledOnce();
  });

  test("Home キーで goFirst が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("Home");
    expect(defaultCallbacks.goFirst).toHaveBeenCalledOnce();
  });

  test("End キーで goLast が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("End");
    expect(defaultCallbacks.goLast).toHaveBeenCalledOnce();
  });

  test("T キーで focusBrowser が呼ばれる", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, true));
    pressKey("t");
    expect(defaultCallbacks.focusBrowser).toHaveBeenCalledOnce();
  });

  test("enabled=false の場合キーが無効化される", () => {
    renderHook(() => useTreeKeyboard(defaultCallbacks, false));
    pressKey("ArrowDown");
    pressKey("ArrowUp");
    pressKey("ArrowRight");
    pressKey("Enter");
    pressKey("t");
    expect(defaultCallbacks.moveDown).not.toHaveBeenCalled();
    expect(defaultCallbacks.moveUp).not.toHaveBeenCalled();
    expect(defaultCallbacks.expand).not.toHaveBeenCalled();
    expect(defaultCallbacks.select).not.toHaveBeenCalled();
    expect(defaultCallbacks.focusBrowser).not.toHaveBeenCalled();
  });
});
