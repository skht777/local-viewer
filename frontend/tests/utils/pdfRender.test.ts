// pdfRender ユーティリティのテスト
// - computePdfRenderScale: fitMode に応じた scale / cacheKey 算出
// - renderPdfPageToCache: オフスクリーン描画してキャッシュへ格納（先読み）

import { vi, describe, test, expect, beforeEach } from "vitest";
import { computePdfRenderScale, renderPdfPageToCache, MAX_SCALE } from "../../src/utils/pdfRender";

function mockPage(baseWidth = 600, baseHeight = 800) {
  return {
    getViewport: ({ scale }: { scale: number }) => ({
      width: baseWidth * scale,
      height: baseHeight * scale,
    }),
    render: vi.fn(() => ({ promise: Promise.resolve() })),
    cleanup: vi.fn(),
  };
}

describe("computePdfRenderScale", () => {
  test("fitMode=width で containerWidth / baseWidth を scale にする", () => {
    const page = mockPage(600, 800);
    const r = computePdfRenderScale({
      page: page as never,
      pageNumber: 1,
      fitMode: "width",
      containerWidth: 1200,
      containerHeight: 9999,
    });
    expect(r.scale).toBe(2);
    expect(r.cacheKey).toBe("1:2");
  });

  test("fitMode=height で containerHeight / baseHeight を scale にする", () => {
    const page = mockPage(600, 800);
    const r = computePdfRenderScale({
      page: page as never,
      pageNumber: 3,
      fitMode: "height",
      containerWidth: 9999,
      containerHeight: 400,
    });
    expect(r.scale).toBe(0.5);
    expect(r.cacheKey).toBe("3:0.5");
  });

  test("scale は MAX_SCALE を超えない", () => {
    const page = mockPage(100, 100);
    const r = computePdfRenderScale({
      page: page as never,
      pageNumber: 1,
      fitMode: "width",
      containerWidth: 100_000,
      containerHeight: 100_000,
    });
    expect(r.scale).toBe(MAX_SCALE);
  });
});

describe("renderPdfPageToCache", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
      clearRect: vi.fn(),
      drawImage: vi.fn(),
    })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    globalThis.createImageBitmap = vi.fn(
      async () => ({ close: vi.fn() }) as unknown as ImageBitmap,
    );
  });

  function mockCache(initial?: ImageBitmap) {
    return {
      get: vi.fn(() => initial),
      put: vi.fn(),
      invalidate: vi.fn(),
    };
  }

  test("ページを描画して cacheKey でキャッシュへ格納する", async () => {
    const page = mockPage(600, 800);
    const doc = { getPage: vi.fn(async () => page) };
    const cache = mockCache();

    await renderPdfPageToCache({
      document: doc as never,
      pageNumber: 2,
      fitMode: "width",
      containerWidth: 1200,
      containerHeight: 9999,
      cache: cache as never,
      isCancelled: () => false,
    });

    expect(page.render).toHaveBeenCalledOnce();
    // cacheKey = "2:2" (scale=1200/600=2, dpr=1)
    expect(cache.put).toHaveBeenCalledWith("2:2", expect.anything());
    expect(page.cleanup).toHaveBeenCalledOnce();
  });

  test("既にキャッシュ済みなら再描画しない", async () => {
    const page = mockPage(600, 800);
    const doc = { getPage: vi.fn(async () => page) };
    const cache = mockCache({ close: vi.fn() } as unknown as ImageBitmap);

    await renderPdfPageToCache({
      document: doc as never,
      pageNumber: 2,
      fitMode: "width",
      containerWidth: 1200,
      containerHeight: 9999,
      cache: cache as never,
      isCancelled: () => false,
    });

    expect(page.render).not.toHaveBeenCalled();
    expect(cache.put).not.toHaveBeenCalled();
  });

  test("コンテナ寸法が 0 以下なら getPage しない", async () => {
    const doc = { getPage: vi.fn() };
    const cache = mockCache();

    await renderPdfPageToCache({
      document: doc as never,
      pageNumber: 2,
      fitMode: "width",
      containerWidth: 0,
      containerHeight: 600,
      cache: cache as never,
      isCancelled: () => false,
    });

    expect(doc.getPage).not.toHaveBeenCalled();
  });

  test("キャンセル済みなら描画後に put しない", async () => {
    const page = mockPage(600, 800);
    const doc = { getPage: vi.fn(async () => page) };
    const cache = mockCache();

    await renderPdfPageToCache({
      document: doc as never,
      pageNumber: 2,
      fitMode: "width",
      containerWidth: 1200,
      containerHeight: 9999,
      cache: cache as never,
      isCancelled: () => true,
    });

    expect(cache.put).not.toHaveBeenCalled();
    expect(page.cleanup).toHaveBeenCalledOnce();
  });
});
