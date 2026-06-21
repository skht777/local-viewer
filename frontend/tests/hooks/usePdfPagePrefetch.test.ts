// usePdfPagePrefetch のテスト
// - 次グループのページを debounce 後に先読みする
// - 見開きモードではグループ単位（複数ページ）を先読みする
// - 末尾グループでは先読みしない

import { renderHook } from "@testing-library/react";
import { vi, describe, test, expect, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/pdfRender", () => ({
  renderPdfPageToCache: vi.fn(async () => {}),
}));

import { renderPdfPageToCache } from "../../src/utils/pdfRender";
import { usePdfPagePrefetch } from "../../src/hooks/usePdfPagePrefetch";
import type { SpreadMode } from "../../src/stores/viewerStore";

const mockRender = vi.mocked(renderPdfPageToCache);
const cache = { get: vi.fn(), put: vi.fn(), invalidate: vi.fn() };

function baseParams(overrides: Record<string, unknown> = {}) {
  return {
    document: {} as never,
    currentPage: 0,
    pageCount: 10,
    spreadMode: "single" as SpreadMode,
    fitMode: "width" as const,
    pageContainerWidth: 800,
    containerHeight: 600,
    renderCache: cache as never,
    ...overrides,
  };
}

describe("usePdfPagePrefetch", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  test("single モードで次ページ(0-based 1, 1-based 2)を debounce 後に先読みする", () => {
    renderHook(() => usePdfPagePrefetch(baseParams()));

    expect(mockRender).not.toHaveBeenCalled();
    vi.advanceTimersByTime(150);

    expect(mockRender).toHaveBeenCalledTimes(1);
    expect(mockRender).toHaveBeenCalledWith(expect.objectContaining({ pageNumber: 2 }));
  });

  test("spread モードでは次グループの全ページ(3,4)を先読みする", () => {
    renderHook(() => usePdfPagePrefetch(baseParams({ spreadMode: "spread", currentPage: 0 })));

    vi.advanceTimersByTime(150);

    // 現在グループ [0,1] → 次グループ [2,3] → 1-based 3,4
    const pages = mockRender.mock.calls.map((c) => (c[0] as { pageNumber: number }).pageNumber);
    expect(pages.toSorted((a, b) => a - b)).toEqual([3, 4]);
  });

  test("末尾グループでは先読みしない", () => {
    renderHook(() => usePdfPagePrefetch(baseParams({ currentPage: 9, pageCount: 10 })));

    vi.advanceTimersByTime(150);
    expect(mockRender).not.toHaveBeenCalled();
  });

  test("document が null なら先読みしない", () => {
    renderHook(() => usePdfPagePrefetch(baseParams({ document: null })));

    vi.advanceTimersByTime(150);
    expect(mockRender).not.toHaveBeenCalled();
  });

  test("debounce 完了前にアンマウントすると先読みしない", () => {
    const { unmount } = renderHook(() => usePdfPagePrefetch(baseParams()));
    unmount();

    vi.advanceTimersByTime(150);
    expect(mockRender).not.toHaveBeenCalled();
  });
});
