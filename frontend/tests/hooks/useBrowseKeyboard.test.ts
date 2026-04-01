import { renderHook } from "@testing-library/react";
import { useBrowseKeyboard } from "../../src/hooks/useBrowseKeyboard";

// react-hotkeys-hook v5 は key と code の両方を見る
const KEY_CODE_MAP: Record<string, string> = {
  ArrowRight: "ArrowRight",
  ArrowLeft: "ArrowLeft",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  Enter: "Enter",
  Escape: "Escape",
  " ": "Space",
  a: "KeyA",
  b: "KeyB",
  d: "KeyD",
  g: "KeyG",
  m: "KeyM",
  n: "KeyN",
  s: "KeyS",
  t: "KeyT",
  u: "KeyU",
  w: "KeyW",
  "1": "Digit1",
  "2": "Digit2",
  "3": "Digit3",
};

function pressKey(key: string, options: KeyboardEventInit = {}) {
  const code = KEY_CODE_MAP[key] ?? key;
  const init: KeyboardEventInit = { key, code, bubbles: true, cancelable: true, ...options };
  document.dispatchEvent(new KeyboardEvent("keydown", init));
  document.dispatchEvent(new KeyboardEvent("keyup", init));
}

describe("useBrowseKeyboard", () => {
  const defaultCallbacks = {
    move: vi.fn(),
    action: vi.fn(),
    open: vi.fn(),
    goParent: vi.fn(),
    focusTree: vi.fn(),
    toggleMode: vi.fn(),
    sortName: vi.fn(),
    sortDate: vi.fn(),
    tabChange: vi.fn(),
    getColumnCount: vi.fn(() => 4),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  // --- 矢印キー / WASD でグリッド移動 ---

  test("→ キーで move(+1) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("ArrowRight");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(1);
  });

  test("← キーで move(-1) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("ArrowLeft");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(-1);
  });

  test("↓ キーで move(+columnCount) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("ArrowDown");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(4);
  });

  test("↑ キーで move(-columnCount) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("ArrowUp");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(-4);
  });

  test("D キーで move(+1) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("d");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(1);
  });

  test("A キーで move(-1) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("a");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(-1);
  });

  test("S キーで move(+columnCount) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("s");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(4);
  });

  test("W キーで move(-columnCount) が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("w");
    expect(defaultCallbacks.move).toHaveBeenCalledWith(-4);
  });

  // --- アクション ---

  test("G キーで action が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("g");
    expect(defaultCallbacks.action).toHaveBeenCalledOnce();
  });

  test("Enter キーで action が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("Enter");
    expect(defaultCallbacks.action).toHaveBeenCalledOnce();
  });

  test("Space キーで open が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey(" ");
    expect(defaultCallbacks.open).toHaveBeenCalledOnce();
  });

  // --- ナビゲーション ---

  test("B キーで goParent が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("b");
    expect(defaultCallbacks.goParent).toHaveBeenCalledOnce();
  });

  test("T キーで focusTree が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("t");
    expect(defaultCallbacks.focusTree).toHaveBeenCalledOnce();
  });

  // --- モード・ソート ---

  test("M キーで toggleMode が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("m");
    expect(defaultCallbacks.toggleMode).toHaveBeenCalledOnce();
  });

  test("N キーで sortName が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("n");
    expect(defaultCallbacks.sortName).toHaveBeenCalledOnce();
  });

  test("U キーで sortDate が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("u");
    expect(defaultCallbacks.sortDate).toHaveBeenCalledOnce();
  });

  // --- タブ切替 ---

  test("1 キーで tabChange('filesets') が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("1");
    expect(defaultCallbacks.tabChange).toHaveBeenCalledWith("filesets");
  });

  test("2 キーで tabChange('images') が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("2");
    expect(defaultCallbacks.tabChange).toHaveBeenCalledWith("images");
  });

  test("3 キーで tabChange('videos') が呼ばれる", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, true));
    pressKey("3");
    expect(defaultCallbacks.tabChange).toHaveBeenCalledWith("videos");
  });

  // --- enabled=false で無効化 ---

  test("enabled=false の場合キーが無効化される", () => {
    renderHook(() => useBrowseKeyboard(defaultCallbacks, false));
    pressKey("ArrowRight");
    pressKey("g");
    pressKey("b");
    pressKey("1");
    expect(defaultCallbacks.move).not.toHaveBeenCalled();
    expect(defaultCallbacks.action).not.toHaveBeenCalled();
    expect(defaultCallbacks.goParent).not.toHaveBeenCalled();
    expect(defaultCallbacks.tabChange).not.toHaveBeenCalled();
  });

  // --- 列数が異なる場合 ---

  test("getColumnCount の戻り値に応じて上下移動量が変わる", () => {
    const callbacks = { ...defaultCallbacks, getColumnCount: vi.fn(() => 3) };
    renderHook(() => useBrowseKeyboard(callbacks, true));
    pressKey("ArrowDown");
    expect(callbacks.move).toHaveBeenCalledWith(3);
  });
});
