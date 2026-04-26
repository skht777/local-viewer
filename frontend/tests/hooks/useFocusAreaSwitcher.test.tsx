// useFocusAreaSwitcher の振る舞い検証
// - focusArea の初期値は "browser"
// - handleFocusTree: data-node-id 一致ノードへ focus、なければ最初の tree-node- へ fallback
// - handleFocusBrowser: aria-current='true' のカードへ focus

import { renderHook, act } from "@testing-library/react";
import { useRef } from "react";
import { useFocusAreaSwitcher } from "../../src/hooks/useFocusAreaSwitcher";

describe("useFocusAreaSwitcher", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  test("初期 focusArea は 'browser'", () => {
    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(null);
      return useFocusAreaSwitcher({ treeRef: ref, nodeId: undefined });
    });
    expect(result.current.focusArea).toBe("browser");
  });

  test("handleFocusTree で focusArea が 'tree' に変わる", () => {
    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(null);
      return useFocusAreaSwitcher({ treeRef: ref, nodeId: "n1" });
    });
    act(() => result.current.handleFocusTree());
    expect(result.current.focusArea).toBe("tree");
  });

  test("handleFocusTree は data-node-id 一致ノードへ focus する", () => {
    const tree = document.createElement("div");
    const target = document.createElement("button");
    target.dataset.nodeId = "n2";
    target.dataset.testid = "tree-node-n2";
    tree.append(target);
    document.body.append(tree);

    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(tree);
      return useFocusAreaSwitcher({ treeRef: ref, nodeId: "n2" });
    });
    act(() => result.current.handleFocusTree());
    expect(document.activeElement).toBe(target);
  });

  test("該当 nodeId が無いときは先頭の tree-node- へ fallback する", () => {
    const tree = document.createElement("div");
    const first = document.createElement("button");
    first.dataset.testid = "tree-node-first";
    const second = document.createElement("button");
    second.dataset.testid = "tree-node-second";
    tree.append(first);
    tree.append(second);
    document.body.append(tree);

    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(tree);
      return useFocusAreaSwitcher({ treeRef: ref, nodeId: "missing" });
    });
    act(() => result.current.handleFocusTree());
    expect(document.activeElement).toBe(first);
  });

  test("handleFocusBrowser は aria-current='true' のカードへ focus し focusArea を 'browser' に戻す", () => {
    const card = document.createElement("button");
    card.setAttribute("aria-current", "true");
    document.body.append(card);

    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(null);
      return useFocusAreaSwitcher({ treeRef: ref, nodeId: undefined });
    });
    act(() => result.current.handleFocusTree());
    expect(result.current.focusArea).toBe("tree");

    act(() => result.current.handleFocusBrowser());
    expect(result.current.focusArea).toBe("browser");
    expect(document.activeElement).toBe(card);
  });
});
