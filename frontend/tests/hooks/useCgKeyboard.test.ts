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
  PageDown: "PageDown",
  PageUp: "PageUp",
  a: "KeyA",
  d: "KeyD",
  f: "KeyF",
  h: "KeyH",
  m: "KeyM",
  q: "KeyQ",
  s: "KeyS",
  v: "KeyV",
  w: "KeyW",
  x: "KeyX",
  z: "KeyZ",
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
    onEscape: vi.fn(),
    toggleFullscreen: vi.fn(),
    setFitWidth: vi.fn(),
    setFitHeight: vi.fn(),
    cycleSpread: vi.fn(),
    scrollUp: vi.fn(),
    scrollDown: vi.fn(),
    goNextSet: vi.fn(),
    goPrevSet: vi.fn(),
    goNextSetParent: vi.fn(),
    goPrevSetParent: vi.fn(),
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

  test("Escape キーで onEscape が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("Escape");
    expect(defaultCallbacks.onEscape).toHaveBeenCalledOnce();
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

  // --- Phase 3: セット間ジャンプ ---

  test("PageDown キーで goNextSet が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("PageDown");
    expect(defaultCallbacks.goNextSet).toHaveBeenCalledOnce();
  });

  test("X キーで goNextSet が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("x");
    expect(defaultCallbacks.goNextSet).toHaveBeenCalledOnce();
  });

  test("PageUp キーで goPrevSet が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("PageUp");
    expect(defaultCallbacks.goPrevSet).toHaveBeenCalledOnce();
  });

  test("Z キーで goPrevSet が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("z");
    expect(defaultCallbacks.goPrevSet).toHaveBeenCalledOnce();
  });

  test("Shift+X で goNextSetParent が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("x", { shiftKey: true });
    expect(defaultCallbacks.goNextSetParent).toHaveBeenCalledOnce();
  });

  test("Shift+Z で goPrevSetParent が呼ばれる", () => {
    renderHook(() => useCgKeyboard(defaultCallbacks));
    pressKey("z", { shiftKey: true });
    expect(defaultCallbacks.goPrevSetParent).toHaveBeenCalledOnce();
  });
});
