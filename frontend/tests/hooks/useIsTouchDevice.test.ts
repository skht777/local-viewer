import { act, renderHook } from "@testing-library/react";
import { useIsTouchDevice } from "../../src/hooks/useIsTouchDevice";

// MQ ハンドラを capture して動的変更をシミュレートできる matchMedia モック
type Listener = (e: { matches: boolean }) => void;

function mockMatchMedia(initialMatches: boolean) {
  let matches = initialMatches;
  const listeners = new Set<Listener>();

  const mql = {
    get matches() {
      return matches;
    },
    media: "(pointer: coarse)",
    addEventListener: (_type: string, listener: Listener) => {
      listeners.add(listener);
    },
    removeEventListener: (_type: string, listener: Listener) => {
      listeners.delete(listener);
    },
  };

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: vi.fn().mockImplementation(() => mql),
  });

  return {
    setMatches: (next: boolean) => {
      matches = next;
      for (const listener of listeners) {
        listener({ matches: next });
      }
    },
  };
}

describe("useIsTouchDevice", () => {
  test("初期 pointer:coarse が false なら false を返す", () => {
    mockMatchMedia(false);
    const { result } = renderHook(() => useIsTouchDevice());
    expect(result.current).toBe(false);
  });

  test("初期 pointer:coarse が true なら true を返す", () => {
    mockMatchMedia(true);
    const { result } = renderHook(() => useIsTouchDevice());
    expect(result.current).toBe(true);
  });

  test("MQ change で false → true に追従する", () => {
    const ctl = mockMatchMedia(false);
    const { result } = renderHook(() => useIsTouchDevice());
    expect(result.current).toBe(false);
    act(() => {
      ctl.setMatches(true);
    });
    expect(result.current).toBe(true);
  });

  test("MQ change で true → false に追従する", () => {
    const ctl = mockMatchMedia(true);
    const { result } = renderHook(() => useIsTouchDevice());
    expect(result.current).toBe(true);
    act(() => {
      ctl.setMatches(false);
    });
    expect(result.current).toBe(false);
  });

  test("matchMedia 未定義の SSR 相当環境で例外を投げず false を返す", () => {
    // matchMedia を一時的に削除
    const original = window.matchMedia;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).matchMedia = undefined;
    try {
      const { result } = renderHook(() => useIsTouchDevice());
      expect(result.current).toBe(false);
    } finally {
      window.matchMedia = original;
    }
  });
});
