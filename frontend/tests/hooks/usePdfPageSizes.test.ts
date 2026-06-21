// usePdfPageSizes フックのテスト
// - 先頭ページのみサンプリングし、全ページの estimateSize 推定値とする（均一PDF前提）
// - 実際の表示ページは virtualizer.measureElement が再計測するため、可変サイズPDFでも
//   スクロール後は補正される。全ページ getPage を避けて初期化を高速化する

import { renderHook, waitFor, act } from "@testing-library/react";
import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
}));

import { usePdfPageSizes } from "../../src/hooks/usePdfPageSizes";

function createMockDocument(pages: { width: number; height: number }[]) {
  return {
    numPages: pages.length,
    getPage: vi.fn((num: number) =>
      Promise.resolve({
        getViewport: ({ scale }: { scale: number }) => ({
          width: pages[num - 1].width * scale,
          height: pages[num - 1].height * scale,
        }),
        cleanup: vi.fn(),
      }),
    ),
    destroy: vi.fn(),
  };
}

describe("usePdfPageSizes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("先頭ページのサイズを全ページの推定値として返す", async () => {
    const mockDoc = createMockDocument([
      { width: 612, height: 792 },
      { width: 842, height: 595 },
      { width: 612, height: 792 },
    ]);

    const { result } = renderHook(() => usePdfPageSizes(mockDoc as never));

    await waitFor(() => {
      expect(result.current.isReady).toBe(true);
    });

    // 全ページ分の長さで、すべて先頭ページ (612x792) のサイズ
    expect(result.current.pageSizes).toHaveLength(3);
    expect(result.current.pageSizes[0]).toEqual({ width: 612, height: 792 });
    expect(result.current.pageSizes[1]).toEqual({ width: 612, height: 792 });
    expect(result.current.pageSizes[2]).toEqual({ width: 612, height: 792 });
  });

  test("getPage は先頭ページのみ呼ぶ（全ページ走査しない）", async () => {
    const pages = Array.from({ length: 50 }, () => ({ width: 612, height: 792 }));
    const mockDoc = createMockDocument(pages);

    const { result } = renderHook(() => usePdfPageSizes(mockDoc as never));

    await waitFor(() => {
      expect(result.current.isReady).toBe(true);
    });

    expect(mockDoc.getPage).toHaveBeenCalledTimes(1);
    expect(mockDoc.getPage).toHaveBeenCalledWith(1);
    expect(result.current.pageSizes).toHaveLength(50);
  });

  test("document=nullではisReady=false", () => {
    const { result } = renderHook(() => usePdfPageSizes(null));

    expect(result.current.isReady).toBe(false);
    expect(result.current.pageSizes).toHaveLength(0);
  });

  test("アンマウント時にサンプリングが中断される", async () => {
    const resolver: { fn?: (page: unknown) => void } = {};
    const mockDoc = {
      numPages: 25,
      getPage: vi.fn(
        () =>
          new Promise((resolve) => {
            resolver.fn = resolve;
          }),
      ),
      destroy: vi.fn(),
    };

    const { result, unmount } = renderHook(() => usePdfPageSizes(mockDoc as never));

    // getPage(1) 解決前にアンマウント
    await act(async () => {
      unmount();
      resolver.fn?.({
        getViewport: () => ({ width: 612, height: 792 }),
        cleanup: vi.fn(),
      });
    });

    // アンマウント後は isReady が true にならない
    expect(result.current.isReady).toBe(false);
  });
});
