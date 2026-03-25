// usePdfPageSizes フックのテスト
// - 全ページの viewport サイズを事前取得し estimateSize に提供

import { renderHook, waitFor } from "@testing-library/react";
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
});
