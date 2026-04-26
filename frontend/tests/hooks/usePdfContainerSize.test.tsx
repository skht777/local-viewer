// usePdfContainerSize の振る舞い検証
// - combinedRef アタッチで clientWidth/Height を反映
// - ResizeObserver の callback で contentRect が更新される
// - 同一サイズの観測では state を更新しない
// - unmount で disconnect される

import { renderHook, act } from "@testing-library/react";
import { usePdfContainerSize } from "../../src/hooks/usePdfContainerSize";

interface ObserverInstance {
  cb: ResizeObserverCallback;
  observed: Element[];
  disconnected: boolean;
}

let observers: ObserverInstance[] = [];

class MockResizeObserver implements ResizeObserver {
  cb: ResizeObserverCallback;
  observed: Element[] = [];
  disconnected = false;
  constructor(cb: ResizeObserverCallback) {
    this.cb = cb;
    observers.push(this);
  }
  observe(el: Element) {
    this.observed.push(el);
  }
  disconnect() {
    this.disconnected = true;
  }
  unobserve() {}
}

beforeEach(() => {
  observers = [];
  globalThis.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;
});

function makeDiv(width: number, height: number): HTMLDivElement {
  const el = document.createElement("div");
  Object.defineProperty(el, "clientWidth", { configurable: true, value: width });
  Object.defineProperty(el, "clientHeight", { configurable: true, value: height });
  return el;
}

describe("usePdfContainerSize", () => {
  test("初期 containerSize は default (800x600)", () => {
    const { result } = renderHook(() => usePdfContainerSize());
    expect(result.current.containerSize).toEqual({ width: 800, height: 600 });
  });

  test("combinedRef アタッチで clientWidth/Height を反映する", () => {
    const { result } = renderHook(() => usePdfContainerSize());
    act(() => {
      result.current.combinedRef(makeDiv(1024, 768));
    });
    expect(result.current.containerSize).toEqual({ width: 1024, height: 768 });
  });

  test("clientWidth が 0 の場合は default サイズが使われる", () => {
    const { result } = renderHook(() => usePdfContainerSize());
    act(() => {
      result.current.combinedRef(makeDiv(0, 0));
    });
    expect(result.current.containerSize).toEqual({ width: 800, height: 600 });
  });

  test("ResizeObserver の callback で contentRect が反映される", () => {
    const { result } = renderHook(() => usePdfContainerSize());
    act(() => {
      result.current.combinedRef(makeDiv(800, 600));
    });
    expect(observers.length).toBeGreaterThan(0);
    act(() => {
      observers.at(-1)!.cb(
        [
          {
            contentRect: { width: 1280, height: 900 } as DOMRectReadOnly,
            target: observers.at(-1)!.observed[0]!,
          } as ResizeObserverEntry,
        ],
        observers.at(-1)! as unknown as ResizeObserver,
      );
    });
    expect(result.current.containerSize).toEqual({ width: 1280, height: 900 });
  });

  test("同一サイズの観測では containerSize 参照が変わらない", () => {
    const { result } = renderHook(() => usePdfContainerSize());
    act(() => {
      result.current.combinedRef(makeDiv(800, 600));
    });
    const before = result.current.containerSize;
    act(() => {
      observers.at(-1)!.cb(
        [
          {
            contentRect: { width: 800, height: 600 } as DOMRectReadOnly,
            target: observers.at(-1)!.observed[0]!,
          } as ResizeObserverEntry,
        ],
        observers.at(-1)! as unknown as ResizeObserver,
      );
    });
    expect(result.current.containerSize).toBe(before);
  });

  test("combinedRef を null に呼ぶと既存 Observer が disconnect される", () => {
    const { result } = renderHook(() => usePdfContainerSize());
    act(() => {
      result.current.combinedRef(makeDiv(800, 600));
    });
    const first = observers.at(-1)!;
    expect(first.disconnected).toBe(false);
    act(() => {
      result.current.combinedRef(null);
    });
    expect(first.disconnected).toBe(true);
  });

  test("imageAreaRef は最後にアタッチした node を保持する", () => {
    const { result } = renderHook(() => usePdfContainerSize());
    const node = makeDiv(800, 600);
    act(() => {
      result.current.combinedRef(node);
    });
    expect(result.current.imageAreaRef.current).toBe(node);
  });

  test("unmount で ResizeObserver が disconnect される", () => {
    const { result, unmount } = renderHook(() => usePdfContainerSize());
    act(() => {
      result.current.combinedRef(makeDiv(800, 600));
    });
    const inst = observers.at(-1)!;
    expect(inst.disconnected).toBe(false);
    unmount();
    expect(inst.disconnected).toBe(true);
  });
});
