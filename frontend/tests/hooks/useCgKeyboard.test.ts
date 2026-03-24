import { renderHook } from "@testing-library/react";
import { useCgKeyboard } from "../../src/hooks/useCgKeyboard";

// react-hotkeys-hook のテストは実際のキーイベントを発火して検証
// jsdom 環境ではキーイベントを document に dispatch する

// react-hotkeys-hook v5 は key と code の両方を見る
const KEY_CODE_MAP: Record<string, string> = {
  ArrowRight: "ArrowRight",
  ArrowLeft: "ArrowLeft",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  Home: "Home",
  End: "End",
  Escape: "Escape",
  a: "KeyA",
  d: "KeyD",
  f: "KeyF",
  h: "KeyH",
  q: "KeyQ",
  s: "KeyS",
  v: "KeyV",
  w: "KeyW",
  m: "KeyM",
};

function pressKey(key: string, options: KeyboardEventInit = {}) {
  const code = KEY_CODE_MAP[key] ?? key;
  const init: KeyboardEventInit = { key, code, bubbles: true, cancelable: true, ...options };
  document.dispatchEvent(new KeyboardEvent("keydown", init));
  document.dispatchEvent(new KeyboardEvent("keyup", init));
}

describe("useCgKeyboard", () => {
  const defaultCallbacks = {
    goNext: vi.fn(),
    goPrev: vi.fn(),
    goFirst: vi.fn(),
    goLast: vi.fn(),
    onClose: vi.fn(),
    toggleFullscreen: vi.fn(),
    setFitWidth: vi.fn(),
    setFitHeight: vi.fn(),
    cycleSpread: vi.fn(),
    scrollUp: vi.fn(),
    scrollDown: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("→ キーで goNext が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("ArrowRight");
    expect(defaultCallbacks.goNext).toHaveBeenCalledOnce();
  });

  test("← キーで goPrev が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("ArrowLeft");
    expect(defaultCallbacks.goPrev).toHaveBeenCalledOnce();
  });

  test("D キーで goNext が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("d");
    expect(defaultCallbacks.goNext).toHaveBeenCalledOnce();
  });

  test("A キーで goPrev が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("a");
    expect(defaultCallbacks.goPrev).toHaveBeenCalledOnce();
  });

  test("Home キーで goFirst が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("Home");
    expect(defaultCallbacks.goFirst).toHaveBeenCalledOnce();
  });

  test("End キーで goLast が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("End");
    expect(defaultCallbacks.goLast).toHaveBeenCalledOnce();
  });

  test("F キーで toggleFullscreen が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("f");
    expect(defaultCallbacks.toggleFullscreen).toHaveBeenCalledOnce();
  });

  test("Escape キーで onClose が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("Escape");
    expect(defaultCallbacks.onClose).toHaveBeenCalledOnce();
  });

  test("V キーで setFitWidth が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("v");
    expect(defaultCallbacks.setFitWidth).toHaveBeenCalledOnce();
  });

  test("H キーで setFitHeight が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("h");
    expect(defaultCallbacks.setFitHeight).toHaveBeenCalledOnce();
  });

  test("Q キーで cycleSpread が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("q");
    expect(defaultCallbacks.cycleSpread).toHaveBeenCalledOnce();
  });

  test("W キーで scrollUp が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("w");
    expect(defaultCallbacks.scrollUp).toHaveBeenCalledOnce();
  });

  test("S キーで scrollDown が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("s");
    expect(defaultCallbacks.scrollDown).toHaveBeenCalledOnce();
  });

  test("M キーで toggleMode が呼ばれる", () => {
    const callbacks = { ...defaultCallbacks, toggleMode: vi.fn() };
    renderHook(() => useCgKeyboard(callbacks));
    pressKey("m");
    expect(callbacks.toggleMode).toHaveBeenCalledOnce();
  });
});
