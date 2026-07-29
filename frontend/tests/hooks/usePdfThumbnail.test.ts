// usePdfThumbnail の振る舞い検証
// - PDF 先頭ページを描画して blob URL を返す
// - 仮想スクロールの再マウント時はキャッシュを再利用し PDF を取り直さない
// - node_id / modified_at が変われば再描画する

import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
}));

import { getDocument } from "../../src/lib/pdfjs";
import { clearPdfThumbnailCache, usePdfThumbnail } from "../../src/hooks/usePdfThumbnail";

function createMockDocument() {
  const page = {
    getViewport: vi.fn(({ scale }: { scale: number }) => ({
      width: 600 * scale,
      height: 800 * scale,
    })),
    render: vi.fn(() => ({ promise: Promise.resolve() })),
  };
  const document_ = {
    getPage: vi.fn(() => Promise.resolve(page)),
    destroy: vi.fn(),
  };
  return { document_, page };
}

let urlCounter = 0;

// jsdom の canvas には toBlob が無いので JPEG blob を即返すスタブを使う
const stubToBlob: typeof HTMLCanvasElement.prototype.toBlob = (emit) => {
  emit(new Blob(["jpeg"], { type: "image/jpeg" }));
};

beforeEach(() => {
  vi.clearAllMocks();
  clearPdfThumbnailCache();
  urlCounter = 0;

  HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
    clearRect: vi.fn(),
    drawImage: vi.fn(),
  })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.toBlob = stubToBlob;

  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn(() => `blob:pdf-${++urlCounter}`),
    revokeObjectURL: vi.fn(),
  });
});

describe("usePdfThumbnail", () => {
  test("先頭ページを描画して blob URL を返す", async () => {
    const { document_ } = createMockDocument();
    vi.mocked(getDocument).mockReturnValue({
      promise: Promise.resolve(document_),
    } as never);

    const { result } = renderHook(() => usePdfThumbnail("pdf-1", true, 100));

    await waitFor(() => {
      expect(result.current.url).toBe("blob:pdf-1");
    });
    expect(document_.getPage).toHaveBeenCalledWith(1);
  });

  test("enabled=false の間は PDF を取得しない", () => {
    renderHook(() => usePdfThumbnail("pdf-1", false, 100));
    expect(getDocument).not.toHaveBeenCalled();
  });

  test("再マウント時はキャッシュを再利用し PDF を再取得しない", async () => {
    // 仮想スクロールで画面外 → 画面内に戻るたびに PDF 全体を取り直すのを防ぐ
    const { document_ } = createMockDocument();
    vi.mocked(getDocument).mockReturnValue({
      promise: Promise.resolve(document_),
    } as never);

    const first = renderHook(() => usePdfThumbnail("pdf-1", true, 100));
    await waitFor(() => {
      expect(first.result.current.url).toBe("blob:pdf-1");
    });
    first.unmount();

    const second = renderHook(() => usePdfThumbnail("pdf-1", true, 100));

    expect(second.result.current.url).toBe("blob:pdf-1");
    expect(second.result.current.isLoading).toBe(false);
    expect(vi.mocked(getDocument)).toHaveBeenCalledTimes(1);
  });

  test("modified_at が変わればキャッシュを使わず再描画する", async () => {
    const { document_ } = createMockDocument();
    vi.mocked(getDocument).mockReturnValue({
      promise: Promise.resolve(document_),
    } as never);

    const first = renderHook(() => usePdfThumbnail("pdf-1", true, 100));
    await waitFor(() => {
      expect(first.result.current.url).toBe("blob:pdf-1");
    });
    first.unmount();

    const second = renderHook(() => usePdfThumbnail("pdf-1", true, 200));
    await waitFor(() => {
      expect(second.result.current.url).toBe("blob:pdf-2");
    });
    expect(vi.mocked(getDocument)).toHaveBeenCalledTimes(2);
  });

  test("描画に失敗したら hasError が true になる", async () => {
    vi.mocked(getDocument).mockReturnValue({
      promise: Promise.reject(new Error("broken pdf")),
    } as never);

    const { result } = renderHook(() => usePdfThumbnail("pdf-broken", true, 100));

    await waitFor(() => {
      expect(result.current.hasError).toBe(true);
    });
    expect(result.current.url).toBeNull();
  });
});
