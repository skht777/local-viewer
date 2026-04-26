// useSearchDropdown の振る舞い検証
// - debouncedQuery が 2 文字以上で isOpen=true、未満で isOpen=false
// - クエリ更新で activeIndex が -1 にリセット
// - containerRef 外クリックで isOpen=false

import { renderHook, act } from "@testing-library/react";
import { useRef } from "react";
import { useSearchDropdown } from "../../src/hooks/useSearchDropdown";

describe("useSearchDropdown", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  test("初期状態は isOpen=false / activeIndex=-1", () => {
    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(null);
      return useSearchDropdown({ debouncedQuery: "", containerRef: ref });
    });
    expect(result.current.isOpen).toBe(false);
    expect(result.current.activeIndex).toBe(-1);
  });

  test("debouncedQuery が 2 文字以上で isOpen=true", () => {
    const { result } = renderHook(
      ({ q }: { q: string }) => {
        const ref = useRef<HTMLElement>(null);
        return useSearchDropdown({ debouncedQuery: q, containerRef: ref });
      },
      { initialProps: { q: "" } },
    );
    expect(result.current.isOpen).toBe(false);
  });

  test("クエリが 2 文字以上に更新されると isOpen=true、activeIndex=-1 にリセットされる", () => {
    const { result, rerender } = renderHook(
      ({ q }: { q: string }) => {
        const ref = useRef<HTMLElement>(null);
        return useSearchDropdown({ debouncedQuery: q, containerRef: ref });
      },
      { initialProps: { q: "ab" } },
    );
    expect(result.current.isOpen).toBe(true);
    expect(result.current.activeIndex).toBe(-1);

    act(() => result.current.setActiveIndex(3));
    expect(result.current.activeIndex).toBe(3);

    rerender({ q: "abc" });
    // 結果更新（クエリ変更）で activeIndex が -1 にリセット
    expect(result.current.activeIndex).toBe(-1);
    expect(result.current.isOpen).toBe(true);
  });

  test("debouncedQuery が 2 文字未満になると isOpen=false", () => {
    const { result, rerender } = renderHook(
      ({ q }: { q: string }) => {
        const ref = useRef<HTMLElement>(null);
        return useSearchDropdown({ debouncedQuery: q, containerRef: ref });
      },
      { initialProps: { q: "abc" } },
    );
    expect(result.current.isOpen).toBe(true);
    rerender({ q: "a" });
    expect(result.current.isOpen).toBe(false);
  });

  test("containerRef 外クリックで isOpen=false に閉じる", () => {
    const container = document.createElement("div");
    document.body.append(container);
    const outside = document.createElement("button");
    document.body.append(outside);

    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(container);
      return useSearchDropdown({ debouncedQuery: "abc", containerRef: ref });
    });
    expect(result.current.isOpen).toBe(true);

    act(() => {
      outside.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    expect(result.current.isOpen).toBe(false);
  });

  test("containerRef 内クリックでは isOpen が維持される", () => {
    const container = document.createElement("div");
    const inner = document.createElement("button");
    container.append(inner);
    document.body.append(container);

    const { result } = renderHook(() => {
      const ref = useRef<HTMLElement>(container);
      return useSearchDropdown({ debouncedQuery: "abc", containerRef: ref });
    });
    expect(result.current.isOpen).toBe(true);

    act(() => {
      inner.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    expect(result.current.isOpen).toBe(true);
  });
});
