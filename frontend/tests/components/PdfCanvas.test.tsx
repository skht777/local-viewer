// PdfCanvas コンポーネントのテスト
// - src/lib/pdfjs をモック
// - RenderTask の cancel/cleanup ライフサイクルを検証

import { render, waitFor } from "@testing-library/react";
import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
}));

import { PdfCanvas } from "../../src/components/PdfCanvas";

// モックファクトリ
function createMockPage(width = 612, height = 792) {
  const renderPromise = Promise.resolve();
  const mockRenderTask = {
    promise: renderPromise,
    cancel: vi.fn(),
  };
  const mockPage = {
    getViewport: vi.fn(({ scale }: { scale: number }) => ({
      width: width * scale,
      height: height * scale,
    })),
    render: vi.fn(() => mockRenderTask),
    cleanup: vi.fn(),
  };
  return { mockPage, mockRenderTask };
}

function createMockDocument(pageCount = 5, width = 612, height = 792) {
  const pages = Array.from({ length: pageCount }, () => createMockPage(width, height));
  const mockDocument = {
    numPages: pageCount,
    getPage: vi.fn((num: number) => Promise.resolve(pages[num - 1].mockPage)),
    destroy: vi.fn(),
  };
  return { mockDocument, pages };
}

describe("PdfCanvas", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // jsdom には canvas getContext がないのでモック
    HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
      clearRect: vi.fn(),
      drawImage: vi.fn(),
    })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
  });

  test("canvasが描画される", async () => {
    const { mockDocument, pages } = createMockDocument();

    const { container } = render(
      <PdfCanvas
        document={mockDocument as never}
        pageNumber={1}
        fitMode="width"
        containerWidth={800}
        containerHeight={600}
      />,
    );

    await waitFor(() => {
      expect(mockDocument.getPage).toHaveBeenCalledWith(1);
    });
    expect(pages[0].mockPage.render).toHaveBeenCalled();
    expect(container.querySelector("canvas")).toBeTruthy();
  });

  test("pageNumber変更時にRenderTaskをcancelして再描画する", async () => {
    const { mockDocument, pages } = createMockDocument();

    const { rerender } = render(
      <PdfCanvas
        document={mockDocument as never}
        pageNumber={1}
        fitMode="width"
        containerWidth={800}
        containerHeight={600}
      />,
    );

    await waitFor(() => {
      expect(pages[0].mockPage.render).toHaveBeenCalled();
    });

    // ページ変更
    rerender(
      <PdfCanvas
        document={mockDocument as never}
        pageNumber={2}
        fitMode="width"
        containerWidth={800}
        containerHeight={600}
      />,
    );

    // 前の RenderTask が cancel される
    expect(pages[0].mockRenderTask.cancel).toHaveBeenCalled();

    await waitFor(() => {
      expect(mockDocument.getPage).toHaveBeenCalledWith(2);
    });
    expect(pages[1].mockPage.render).toHaveBeenCalled();
  });

  test("描画完了時にonRenderCompleteが呼ばれる", async () => {
    const { mockDocument } = createMockDocument();
    const onComplete = vi.fn();

    render(
      <PdfCanvas
        document={mockDocument as never}
        pageNumber={1}
        fitMode="width"
        containerWidth={800}
        containerHeight={600}
        onRenderComplete={onComplete}
      />,
    );

    await waitFor(() => {
      expect(onComplete).toHaveBeenCalledOnce();
    });
  });

  test("アンマウント時にRenderTaskをcancelする", async () => {
    // render が永遠に解決しない RenderTask を作成
    const neverResolve = new Promise<void>(() => {});
    const mockRenderTask = { promise: neverResolve, cancel: vi.fn() };
    const mockPage = {
      getViewport: vi.fn(({ scale }: { scale: number }) => ({
        width: 612 * scale,
        height: 792 * scale,
      })),
      render: vi.fn(() => mockRenderTask),
      cleanup: vi.fn(),
    };
    const mockDocument = {
      numPages: 1,
      getPage: vi.fn(() => Promise.resolve(mockPage)),
      destroy: vi.fn(),
    };

    const { unmount } = render(
      <PdfCanvas
        document={mockDocument as never}
        pageNumber={1}
        fitMode="width"
        containerWidth={800}
        containerHeight={600}
      />,
    );

    await waitFor(() => {
      expect(mockPage.render).toHaveBeenCalled();
    });

    unmount();

    expect(mockRenderTask.cancel).toHaveBeenCalledOnce();
  });
});
