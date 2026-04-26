// useFileBrowserKeyboardBindings の振る舞い検証
// - useBrowseKeyboard をモックし、bindings として渡された callbacks の挙動を検証
// - move(delta): indexMap で解決し、setLocalSelectedId + scrollToItem + focus 遷移
// - action: 選択中エントリで handleAction
// - open: 選択中エントリで handleOpen

import { renderHook } from "@testing-library/react";
import { useFileBrowserKeyboardBindings } from "../../src/hooks/useFileBrowserKeyboardBindings";
import type { BrowseEntry } from "../../src/types/api";

// useBrowseKeyboard をモック化し、渡された bindings を捕捉
interface Bindings {
  move: (delta: number) => void;
  action: () => void;
  open: () => void;
  goParent: () => void;
  focusTree: () => void;
  toggleMode: () => void;
  sortName: () => void;
  sortDate: () => void;
  tabChange: (tab: string) => void;
  getColumnCount: () => number;
}

let lastBindings: Bindings | null = null;
let lastEnabled: boolean | undefined = undefined;

vi.mock("../../src/hooks/useBrowseKeyboard", () => ({
  useBrowseKeyboard: (bindings: Bindings, enabled: boolean) => {
    lastBindings = bindings;
    lastEnabled = enabled;
  },
}));

function makeEntry(kind: BrowseEntry["kind"], id: string): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

describe("useFileBrowserKeyboardBindings", () => {
  beforeEach(() => {
    lastBindings = null;
    lastEnabled = undefined;
    document.body.replaceChildren();
  });

  test("keyboardEnabled が useBrowseKeyboard に伝搬する", () => {
    renderHook(() =>
      useFileBrowserKeyboardBindings({
        filtered: [],
        indexMap: new Map(),
        effectiveSelectedId: null,
        scrollToItem: vi.fn(),
        setLocalSelectedId: vi.fn(),
        handleAction: vi.fn(),
        handleOpen: vi.fn(),
        getColumnCount: () => 4,
        keyboardEnabled: false,
      }),
    );
    expect(lastEnabled).toBe(false);
  });

  test("move(delta) は indexMap で解決した位置から delta 分だけ進む", () => {
    const filtered = [makeEntry("image", "a"), makeEntry("image", "b"), makeEntry("image", "c")];
    const indexMap = new Map([
      ["a", 0],
      ["b", 1],
      ["c", 2],
    ]);
    const setLocalSelectedId = vi.fn();
    const scrollToItem = vi.fn();
    renderHook(() =>
      useFileBrowserKeyboardBindings({
        filtered,
        indexMap,
        effectiveSelectedId: "a",
        scrollToItem,
        setLocalSelectedId,
        handleAction: vi.fn(),
        handleOpen: vi.fn(),
        getColumnCount: () => 4,
        keyboardEnabled: true,
      }),
    );
    lastBindings!.move(1);
    expect(setLocalSelectedId).toHaveBeenCalledWith("b");
    expect(scrollToItem).toHaveBeenCalledWith(1);
  });

  test("move が 範囲外なら何も呼ばない", () => {
    const filtered = [makeEntry("image", "a")];
    const indexMap = new Map([["a", 0]]);
    const setLocalSelectedId = vi.fn();
    const scrollToItem = vi.fn();
    renderHook(() =>
      useFileBrowserKeyboardBindings({
        filtered,
        indexMap,
        effectiveSelectedId: "a",
        scrollToItem,
        setLocalSelectedId,
        handleAction: vi.fn(),
        handleOpen: vi.fn(),
        getColumnCount: () => 4,
        keyboardEnabled: true,
      }),
    );
    lastBindings!.move(1);
    expect(setLocalSelectedId).not.toHaveBeenCalled();
    expect(scrollToItem).not.toHaveBeenCalled();
  });

  test("action は effectiveSelectedId のエントリで handleAction を呼ぶ", () => {
    const filtered = [makeEntry("image", "a"), makeEntry("directory", "b")];
    const indexMap = new Map([
      ["a", 0],
      ["b", 1],
    ]);
    const handleAction = vi.fn();
    renderHook(() =>
      useFileBrowserKeyboardBindings({
        filtered,
        indexMap,
        effectiveSelectedId: "b",
        scrollToItem: vi.fn(),
        setLocalSelectedId: vi.fn(),
        handleAction,
        handleOpen: vi.fn(),
        getColumnCount: () => 4,
        keyboardEnabled: true,
      }),
    );
    lastBindings!.action();
    expect(handleAction).toHaveBeenCalledWith(filtered[1]);
  });

  test("open は effectiveSelectedId のエントリで handleOpen を呼ぶ", () => {
    const filtered = [makeEntry("image", "a"), makeEntry("archive", "b")];
    const indexMap = new Map([
      ["a", 0],
      ["b", 1],
    ]);
    const handleOpen = vi.fn();
    renderHook(() =>
      useFileBrowserKeyboardBindings({
        filtered,
        indexMap,
        effectiveSelectedId: "b",
        scrollToItem: vi.fn(),
        setLocalSelectedId: vi.fn(),
        handleAction: vi.fn(),
        handleOpen,
        getColumnCount: () => 4,
        keyboardEnabled: true,
      }),
    );
    lastBindings!.open();
    expect(handleOpen).toHaveBeenCalledWith(filtered[1]);
  });

  test("選択 ID が filtered に存在しないとき action/open は何もしない", () => {
    const handleAction = vi.fn();
    const handleOpen = vi.fn();
    renderHook(() =>
      useFileBrowserKeyboardBindings({
        filtered: [makeEntry("image", "a")],
        indexMap: new Map([["a", 0]]),
        effectiveSelectedId: "missing",
        scrollToItem: vi.fn(),
        setLocalSelectedId: vi.fn(),
        handleAction,
        handleOpen,
        getColumnCount: () => 4,
        keyboardEnabled: true,
      }),
    );
    lastBindings!.action();
    lastBindings!.open();
    expect(handleAction).not.toHaveBeenCalled();
    expect(handleOpen).not.toHaveBeenCalled();
  });

  test("optional callbacks (onGoParent / onFocusTree / onToggleMode 等) が無くても useBrowseKeyboard には no-op が渡される", () => {
    renderHook(() =>
      useFileBrowserKeyboardBindings({
        filtered: [],
        indexMap: new Map(),
        effectiveSelectedId: null,
        scrollToItem: vi.fn(),
        setLocalSelectedId: vi.fn(),
        handleAction: vi.fn(),
        handleOpen: vi.fn(),
        getColumnCount: () => 4,
        keyboardEnabled: true,
      }),
    );
    // 各 optional callback は () => {} に置き換わっており例外を投げない
    expect(() => lastBindings!.goParent()).not.toThrow();
    expect(() => lastBindings!.focusTree()).not.toThrow();
    expect(() => lastBindings!.toggleMode()).not.toThrow();
    expect(() => lastBindings!.sortName()).not.toThrow();
    expect(() => lastBindings!.sortDate()).not.toThrow();
    expect(() => lastBindings!.tabChange("images")).not.toThrow();
  });
});
