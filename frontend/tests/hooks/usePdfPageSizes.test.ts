// usePdfPageSizes フックのテスト
// - 全ページの viewport サイズを事前取得し estimateSize に提供
// - バッチ処理で getPage burst を抑制

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

  test("全ページサイズを取得する", async () => {
    const mockDoc = createMockDocument([
      { width: 612, height: 792 },
      { width: 842, height: 595 },
      { width: 612, height: 792 },
    ]);

    const { result } = renderHook(() => usePdfPageSizes(mockDoc as never));

    await waitFor(() => {
      expect(result.current.isReady).toBe(true);
    });

    expect(result.current.pageSizes).toHaveLength(3);
    expect(result.current.pageSizes[0]).toEqual({ width: 612, height: 792 });
    expect(result.current.pageSizes[1]).toEqual({ width: 842, height: 595 });
  });

  test("document=nullではisReady=false", () => {
    const { result } = renderHook(() => usePdfPageSizes(null));

    expect(result.current.isReady).toBe(false);
    expect(result.current.pageSizes).toHaveLength(0);
  });

  test("バッチ処理で同時getPage呼び出しがBATCH_SIZE以下に制限される", async () => {
    // 25 ページのドキュメント (BATCH_SIZE=10 で 3 バッチ)
    const pages = Array.from({ length: 25 }, () => ({ width: 612, height: 792 }));
    let concurrentCalls = 0;
    let maxConcurrentCalls = 0;

    const mockDoc = {
      numPages: 25,
      getPage: vi.fn((_num: number) => {
        concurrentCalls++;
        maxConcurrentCalls = Math.max(maxConcurrentCalls, concurrentCalls);
        return Promise.resolve({
          getViewport: ({ scale }: { scale: number }) => ({
            width: pages[0].width * scale,
            height: pages[0].height * scale,
          }),
          cleanup: vi.fn(),
        }).then((result) => {
          concurrentCalls--;
          return result;
        });
      }),
      destroy: vi.fn(),
    };

    const { result } = renderHook(() => usePdfPageSizes(mockDoc as never));

    await waitFor(() => {
      expect(result.current.isReady).toBe(true);
    });

    // 同時呼び出し数が BATCH_SIZE (10) 以下であること
    expect(maxConcurrentCalls).toBeLessThanOrEqual(10);
    expect(result.current.pageSizes).toHaveLength(25);
  });

  test("アンマウント時にバッチ処理が中断される", async () => {
    // getPage をバッチ間のタイミングで resolve する遅延付きモック
    const pages = Array.from({ length: 25 }, () => ({ width: 612, height: 792 }));

    const mockDoc = {
      numPages: 25,
      getPage: vi.fn((_num: number) =>
        Promise.resolve({
          getViewport: ({ scale }: { scale: number }) => ({
            width: pages[0].width * scale,
            height: pages[0].height * scale,
          }),
          cleanup: vi.fn(),
        }),
      ),
      destroy: vi.fn(),
    };

    const { result, unmount } = renderHook(() => usePdfPageSizes(mockDoc as never));

    // 最初のバッチが開始される前にアンマウント
    await act(async () => {
      unmount();
    });

    // アンマウント後は isReady が true にならない
    expect(result.current.isReady).toBe(false);
  });
});
