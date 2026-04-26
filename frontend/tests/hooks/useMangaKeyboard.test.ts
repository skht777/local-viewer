import { renderHook } from "@testing-library/react";
import { useMangaKeyboard } from "../../src/hooks/useMangaKeyboard";

const KEY_CODE_MAP: Record<string, string> = {
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  Home: "Home",
  End: "End",
  Escape: "Escape",
  PageDown: "PageDown",
  PageUp: "PageUp",
  f: "KeyF",
  m: "KeyM",
  s: "KeyS",
  w: "KeyW",
  x: "KeyX",
  z: "KeyZ",
  "=": "Equal",
  "-": "Minus",
  "0": "Digit0",
};

function pressKey(key: string, options: KeyboardEventInit = {}) {
  const code = KEY_CODE_MAP[key] ?? key;
  const init: KeyboardEventInit = { key, code, bubbles: true, cancelable: true, ...options };
  document.dispatchEvent(new KeyboardEvent("keydown", init));
  document.dispatchEvent(new KeyboardEvent("keyup", init));
}

describe("useMangaKeyboard", () => {
  const defaultCallbacks = {
    scrollUp: vi.fn(),
    scrollDown: vi.fn(),
    scrollToTop: vi.fn(),
    scrollToBottom: vi.fn(),
    onEscape: vi.fn(),
    toggleFullscreen: vi.fn(),
    goNextSet: vi.fn(),
    goPrevSet: vi.fn(),
    goNextSetParent: vi.fn(),
    goPrevSetParent: vi.fn(),
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    zoomReset: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("↑ キーで scrollUp が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("ArrowUp");
    expect(defaultCallbacks.scrollUp).toHaveBeenCalledOnce();
  });

  test("↓ キーで scrollDown が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("ArrowDown");
    expect(defaultCallbacks.scrollDown).toHaveBeenCalledOnce();
  });

  test("W キーで scrollUp が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("w");
    expect(defaultCallbacks.scrollUp).toHaveBeenCalledOnce();
  });

  test("S キーで scrollDown が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("s");
    expect(defaultCallbacks.scrollDown).toHaveBeenCalledOnce();
  });

  test("Home キーで scrollToTop が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("Home");
    expect(defaultCallbacks.scrollToTop).toHaveBeenCalledOnce();
  });

  test("End キーで scrollToBottom が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("End");
    expect(defaultCallbacks.scrollToBottom).toHaveBeenCalledOnce();
  });

  test("F キーで toggleFullscreen が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("f");
    expect(defaultCallbacks.toggleFullscreen).toHaveBeenCalledOnce();
  });

  test("Escape キーで onEscape が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("Escape");
    expect(defaultCallbacks.onEscape).toHaveBeenCalledOnce();
  });

  test("PageDown キーで goNextSet が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("PageDown");
    expect(defaultCallbacks.goNextSet).toHaveBeenCalledOnce();
  });

  test("X キーで goNextSet が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("x");
    expect(defaultCallbacks.goNextSet).toHaveBeenCalledOnce();
  });

  test("Shift+X で goNextSetParent が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("x", { shiftKey: true });
    expect(defaultCallbacks.goNextSetParent).toHaveBeenCalledOnce();
  });

  test("Shift+Z で goPrevSetParent が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("z", { shiftKey: true });
    expect(defaultCallbacks.goPrevSetParent).toHaveBeenCalledOnce();
  });

  test("- キーで zoomOut が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("-");
    expect(defaultCallbacks.zoomOut).toHaveBeenCalledOnce();
  });

  test("0 キーで zoomReset が呼ばれる", () => {
    renderHook(() => useMangaKeyboard(defaultCallbacks));
    pressKey("0");
    expect(defaultCallbacks.zoomReset).toHaveBeenCalledOnce();
  });

  test("M キーで showTitle が呼ばれる", () => {
    const showTitle = vi.fn();
    renderHook(() => useMangaKeyboard({ ...defaultCallbacks, showTitle }));
    pressKey("m");
    expect(showTitle).toHaveBeenCalledOnce();
  });
});
