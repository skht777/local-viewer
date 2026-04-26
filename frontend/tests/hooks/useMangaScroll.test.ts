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

describe("useMangaScroll", () => {
  test("scrollToImage で virtualizer.scrollToIndex が呼ばれる", () => {
    const virtualizer = createMockVirtualizer();
    const scrollElement = createMockScrollElement();
    const { result } = renderHook(() =>
      useMangaScroll({
        virtualizer: virtualizer as any,
        scrollElement,
        totalCount: 10,
        scrollSpeed: 1.0,
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
        scrollSpeed: 1.0,
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
        scrollSpeed: 1.0,
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
        scrollSpeed: 2.0,
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
        scrollSpeed: 1.0,
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
        scrollSpeed: 1.0,
      }),
    );
    act(() => result.current.scrollToBottom());
    expect(virtualizer.scrollToIndex).toHaveBeenCalledWith(
      9,
      expect.objectContaining({ align: "end" }),
    );
  });
});
