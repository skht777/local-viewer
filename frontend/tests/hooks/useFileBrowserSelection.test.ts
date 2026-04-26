// useFileBrowserSelection の振る舞い検証
// - 優先順位: localSelectedId > selectedNodeId > 先頭エントリ
// - entries 変更時にローカル選択がリセットされる
// - Escape / 外クリックで選択解除

import { renderHook, act } from "@testing-library/react";
import { useFileBrowserSelection } from "../../src/hooks/useFileBrowserSelection";
import type { BrowseEntry } from "../../src/types/api";

function makeEntry(id: string): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind: "image",
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

describe("useFileBrowserSelection", () => {
  test("初期状態は先頭エントリが selected になる", () => {
    const filtered = [makeEntry("a"), makeEntry("b")];
    const { result } = renderHook(() => useFileBrowserSelection({ filtered }));
    expect(result.current.effectiveSelectedId).toBe("a");
  });

  test("selectedNodeId プロップが localSelectedId 未設定時に優先される", () => {
    const filtered = [makeEntry("a"), makeEntry("b")];
    const { result } = renderHook(() => useFileBrowserSelection({ filtered, selectedNodeId: "b" }));
    expect(result.current.effectiveSelectedId).toBe("b");
  });

  test("handleSelect 後は localSelectedId が selectedNodeId プロップより優先される", () => {
    const filtered = [makeEntry("a"), makeEntry("b"), makeEntry("c")];
    const { result } = renderHook(() => useFileBrowserSelection({ filtered, selectedNodeId: "b" }));
    act(() => {
      result.current.handleSelect(filtered[2]!);
    });
    expect(result.current.effectiveSelectedId).toBe("c");
  });

  test("entries の先頭が変わるとローカル選択がリセットされる", () => {
    const initial = [makeEntry("a"), makeEntry("b")];
    const { result, rerender } = renderHook(
      ({ filtered }: { filtered: BrowseEntry[] }) => useFileBrowserSelection({ filtered }),
      { initialProps: { filtered: initial } },
    );
    act(() => {
      result.current.handleSelect(initial[1]!);
    });
    expect(result.current.effectiveSelectedId).toBe("b");

    rerender({ filtered: [makeEntry("x"), makeEntry("y")] });
    expect(result.current.effectiveSelectedId).toBe("x");
  });

  test("Escape キーで localSelectedId が解除され selectedNodeId にフォールバックする", () => {
    const filtered = [makeEntry("a"), makeEntry("b")];
    const { result } = renderHook(() => useFileBrowserSelection({ filtered, selectedNodeId: "b" }));
    act(() => {
      result.current.handleSelect(filtered[0]!);
    });
    expect(result.current.effectiveSelectedId).toBe("a");

    act(() => {
      result.current.handleKeyDown({ key: "Escape" } as React.KeyboardEvent);
    });
    expect(result.current.effectiveSelectedId).toBe("b");
  });

  test("Escape 以外のキーは selection に影響しない", () => {
    const filtered = [makeEntry("a"), makeEntry("b")];
    const { result } = renderHook(() => useFileBrowserSelection({ filtered }));
    act(() => {
      result.current.handleSelect(filtered[1]!);
    });
    act(() => {
      result.current.handleKeyDown({ key: "Enter" } as React.KeyboardEvent);
    });
    expect(result.current.effectiveSelectedId).toBe("b");
  });

  test("外クリック (target === currentTarget) で localSelectedId が解除される", () => {
    const filtered = [makeEntry("a"), makeEntry("b")];
    const { result } = renderHook(() => useFileBrowserSelection({ filtered }));
    act(() => {
      result.current.handleSelect(filtered[1]!);
    });
    expect(result.current.effectiveSelectedId).toBe("b");

    const node = document.createElement("div");
    act(() => {
      result.current.handleMainClick({
        target: node,
        currentTarget: node,
      } as unknown as React.MouseEvent);
    });
    expect(result.current.effectiveSelectedId).toBe("a");
  });

  test("カード内クリック (target !== currentTarget) では選択が解除されない", () => {
    const filtered = [makeEntry("a"), makeEntry("b")];
    const { result } = renderHook(() => useFileBrowserSelection({ filtered }));
    act(() => {
      result.current.handleSelect(filtered[1]!);
    });
    const inner = document.createElement("span");
    const outer = document.createElement("div");
    act(() => {
      result.current.handleMainClick({
        target: inner,
        currentTarget: outer,
      } as unknown as React.MouseEvent);
    });
    expect(result.current.effectiveSelectedId).toBe("b");
  });

  test("filtered が空の場合は effectiveSelectedId が null になる", () => {
    const { result } = renderHook(() => useFileBrowserSelection({ filtered: [] }));
    expect(result.current.effectiveSelectedId).toBeNull();
  });
});
