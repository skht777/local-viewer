import { renderHook, act } from "@testing-library/react";
import { useMangaScroll } from "../../src/hooks/useMangaScroll";

// virtualizer のモック
function createMockVirtualizer() {
  return {
    scrollToIndex: vi.fn(),
    getVirtualItems: vi.fn().mockReturnValue([]),
  };
}

// scrollElement のモック（addEventListener/removeEventListener 付き）
function createMockScrollElement(scrollTop = 0, clientHeight = 800) {
  return {
    scrollTop,
    clientHeight,
    scrollBy: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  } as unknown as HTMLDivElement;
}

// 登録されたリスナーを保持し、テストから任意のイベントを発火できる scrollElement モック
function createListenableScrollElement(scrollTop = 0, clientHeight = 800) {
  const listeners = new Map<string, Set<() => void>>();
  const element = {
    scrollTop,
    clientHeight,
    scrollBy: vi.fn(),
    addEventListener: (type: string, handler: () => void) => {
      const handlers = listeners.get(type) ?? new Set<() => void>();
      handlers.add(handler);
      listeners.set(type, handlers);
    },
    removeEventListener: (type: string, handler: () => void) => {
      listeners.get(type)?.delete(handler);
    },
  };
  const dispatch = (type: string) => {
    for (const handler of listeners.get(type) ?? new Set<() => void>()) {
      handler();
    }
  };
  return { element: element as unknown as HTMLDivElement, dispatch };
}

// 高さ ITEM_SIZE の仮想アイテムを count 件返す virtualizer モック
const ITEM_SIZE = 1000;
function createMockVirtualizerWithItems(count: number) {
  const items = Array.from({ length: count }, (_, index) => ({
    index,
    start: index * ITEM_SIZE,
    size: ITEM_SIZE,
  }));
  return {
    scrollToIndex: vi.fn(),
    getVirtualItems: vi.fn().mockReturnValue(items),
  };
}

describe("useMangaScroll", () => {
  test("scrollToImage で virtualizer.scrollToIndex が呼ばれる", () => {
    const virtualizer = createMockVirtualizer();
    const scrollElement = createMockScrollElement();
    const { result } = renderHook(() =>
      useMangaScroll({
        virtualizer: virtualizer as any,
        scrollElement,
        totalCount: 10,
        scrollSpeed: 1,
      }),
    );
    act(() => result.current.scrollToImage(5));
    expect(virtualizer.scrollToIndex).toHaveBeenCalledWith(
      5,
      expect.objectContaining({ align: "start" }),
    );
  });

  test("scrollDown で scrollBy が正の値で呼ばれる", () => {
    const virtualizer = createMockVirtualizer();
    const scrollElement = createMockScrollElement();
    const { result } = renderHook(() =>
      useMangaScroll({
        virtualizer: virtualizer as any,
        scrollElement,
        totalCount: 10,
        scrollSpeed: 1,
      }),
    );
    act(() => result.current.scrollDown());
    expect(scrollElement.scrollBy).toHaveBeenCalledWith(0, 200);
  });

  test("scrollUp で scrollBy が負の値で呼ばれる", () => {
    const virtualizer = createMockVirtualizer();
    const scrollElement = createMockScrollElement();
    const { result } = renderHook(() =>
      useMangaScroll({
        virtualizer: virtualizer as any,
        scrollElement,
        totalCount: 10,
        scrollSpeed: 1,
      }),
    );
    act(() => result.current.scrollUp());
    expect(scrollElement.scrollBy).toHaveBeenCalledWith(0, -200);
  });

  test("scrollSpeed が scrollBy の量に反映される", () => {
    const virtualizer = createMockVirtualizer();
    const scrollElement = createMockScrollElement();
    const { result } = renderHook(() =>
      useMangaScroll({
        virtualizer: virtualizer as any,
        scrollElement,
        totalCount: 10,
        scrollSpeed: 2,
      }),
    );
    act(() => result.current.scrollDown());
    expect(scrollElement.scrollBy).toHaveBeenCalledWith(0, 400);
  });

  test("scrollToTop で scrollTop が 0 に設定される", () => {
    const virtualizer = createMockVirtualizer();
    const scrollElement = createMockScrollElement(500);
    const { result } = renderHook(() =>
      useMangaScroll({
        virtualizer: virtualizer as any,
        scrollElement,
        totalCount: 10,
        scrollSpeed: 1,
      }),
    );
    act(() => result.current.scrollToTop());
    expect(scrollElement.scrollTop).toBe(0);
  });

  test("scrollToBottom で末尾にスクロールする", () => {
    const virtualizer = createMockVirtualizer();
    const scrollElement = createMockScrollElement();
    const { result } = renderHook(() =>
      useMangaScroll({
        virtualizer: virtualizer as any,
        scrollElement,
        totalCount: 10,
        scrollSpeed: 1,
      }),
    );
    act(() => result.current.scrollToBottom());
    expect(virtualizer.scrollToIndex).toHaveBeenCalledWith(
      9,
      expect.objectContaining({ align: "end" }),
    );
  });

  // V5 回帰: smooth スクロール中は途中経過の位置から index を再計算しない。
  // 抑制しないとスライダーのサムが目標値と現在値の間で跳ねる。
  describe("プログラムスクロール中の index 抑制", () => {
    beforeEach(() => {
      vi.useFakeTimers({
        toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame", "cancelAnimationFrame"],
      });
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    function setup() {
      const virtualizer = createMockVirtualizerWithItems(10);
      const { element, dispatch } = createListenableScrollElement();
      const { result } = renderHook(() =>
        useMangaScroll({
          virtualizer: virtualizer as any,
          scrollElement: element,
          totalCount: 10,
          scrollSpeed: 1,
        }),
      );
      // index 2 が画面中央に来る位置へスクロールしたことにして scroll イベントを流す
      // (scrollTop 2000 + clientHeight 800 / 2 = 2400 → items[2] の範囲 2000..3000)
      const scrollToIndex2 = () => {
        act(() => {
          (element as unknown as { scrollTop: number }).scrollTop = 2000;
          dispatch("scroll");
          vi.advanceTimersByTime(20);
        });
      };
      return { result, dispatch, scrollToIndex2 };
    }

    test("scrollToImage 直後に currentIndex が目標値へ即座に確定する", () => {
      const { result } = setup();
      act(() => result.current.scrollToImage(7));
      expect(result.current.currentIndex).toBe(7);
    });

    test("smooth スクロールの途中経過では currentIndex が上書きされない", () => {
      const { result, scrollToIndex2 } = setup();
      act(() => result.current.scrollToImage(7));
      scrollToIndex2();
      expect(result.current.currentIndex).toBe(7);
    });

    test("scrollend で抑制が解除されスクロール位置への追従に戻る", () => {
      const { result, dispatch, scrollToIndex2 } = setup();
      act(() => result.current.scrollToImage(7));
      act(() => dispatch("scrollend"));
      scrollToIndex2();
      expect(result.current.currentIndex).toBe(2);
    });

    test("ユーザーのホイール操作が割り込むと即座に追従を再開する", () => {
      const { result, dispatch, scrollToIndex2 } = setup();
      act(() => result.current.scrollToImage(7));
      act(() => dispatch("wheel"));
      scrollToIndex2();
      expect(result.current.currentIndex).toBe(2);
    });

    test("ユーザーのタッチ操作が割り込むと即座に追従を再開する", () => {
      const { result, dispatch, scrollToIndex2 } = setup();
      act(() => result.current.scrollToImage(7));
      act(() => dispatch("touchstart"));
      scrollToIndex2();
      expect(result.current.currentIndex).toBe(2);
    });

    test("scrollend が発火しなくてもタイムアウトで抑制が解除される", () => {
      const { result, scrollToIndex2 } = setup();
      act(() => result.current.scrollToImage(7));
      act(() => {
        vi.advanceTimersByTime(1000);
      });
      scrollToIndex2();
      expect(result.current.currentIndex).toBe(2);
    });

    test("キーボードスクロール(scrollDown)でも抑制が解除される", () => {
      const { result, scrollToIndex2 } = setup();
      act(() => result.current.scrollToImage(7));
      act(() => result.current.scrollDown());
      scrollToIndex2();
      expect(result.current.currentIndex).toBe(2);
    });

    test("アンマウント時にフォールバックタイマーが破棄される", () => {
      const virtualizer = createMockVirtualizerWithItems(10);
      const { element } = createListenableScrollElement();
      const { result, unmount } = renderHook(() =>
        useMangaScroll({
          virtualizer: virtualizer as any,
          scrollElement: element,
          totalCount: 10,
          scrollSpeed: 1,
        }),
      );
      act(() => result.current.scrollToImage(7));
      expect(vi.getTimerCount()).toBeGreaterThan(0);
      unmount();
      expect(vi.getTimerCount()).toBe(0);
    });
  });
});
