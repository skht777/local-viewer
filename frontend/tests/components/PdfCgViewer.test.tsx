// PdfCgViewer コンポーネントのテスト

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi, describe, test, expect, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

// src/lib/pdfjs をモック
vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
}));

import { getDocument } from "../../src/lib/pdfjs";
import { PdfCgViewer } from "../../src/components/PdfCgViewer";

const mockGetDocument = vi.mocked(getDocument);

function createMockLoadingTask(pageCount = 5) {
  const renderPromise = Promise.resolve();
  const mockPage = {
    getViewport: vi.fn(({ scale }: { scale: number }) => ({
      width: 612 * scale,
      height: 792 * scale,
    })),
    render: vi.fn(() => ({ promise: renderPromise, cancel: vi.fn() })),
    cleanup: vi.fn(),
  };
  const mockDocument = {
    numPages: pageCount,
    getPage: vi.fn(() => Promise.resolve(mockPage)),
    destroy: vi.fn(),
  };
  const loadingTask = {
    promise: Promise.resolve(mockDocument),
    destroy: vi.fn(),
  };
  return { loadingTask, mockDocument, mockPage };
}

function createErrorLoadingTask(message: string) {
  return {
    promise: Promise.reject(new Error(message)),
    destroy: vi.fn(),
  };
}

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

function defaultProps(overrides: Partial<React.ComponentProps<typeof PdfCgViewer>> = {}) {
  return {
    pdfNodeId: "pdf123",
    pdfName: "test.pdf",
    parentNodeId: "dir456",
    initialPage: 1,
    mode: "cg" as const,
    onPageChange: vi.fn(),
    onModeChange: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
}

describe("PdfCgViewer", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
      clearRect: vi.fn(),
      drawImage: vi.fn(),
    })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    // jsdom には scrollIntoView がない
    Element.prototype.scrollIntoView = vi.fn();
  });

  test("PDF読み込み中にローディング表示", () => {
    // 永遠に解決しない loading task
    const loadingTask = {
      promise: new Promise(() => {}),
      destroy: vi.fn(),
    };
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfCgViewer {...defaultProps()} />);

    expect(screen.getByTestId("pdf-loading")).toBeTruthy();
  });

  test("読み込み完了後にPdfCanvasが表示される", async () => {
    const { loadingTask } = createMockLoadingTask(3);
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfCgViewer {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-cg-page-area")).toBeTruthy();
    });
  });

  test("読み込みエラー時にエラーメッセージを表示", async () => {
    const loadingTask = createErrorLoadingTask("Corrupt PDF");
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfCgViewer {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-error")).toBeTruthy();
    });
    expect(screen.getByText(/Corrupt PDF/)).toBeTruthy();
  });

  test("閉じるボタンでonCloseが呼ばれる", async () => {
    const { loadingTask } = createMockLoadingTask();
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);
    const onClose = vi.fn();

    renderWithProviders(<PdfCgViewer {...defaultProps({ onClose })} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-cg-page-area")).toBeTruthy();
    });

    await userEvent.click(screen.getByLabelText("閉じる"));
    expect(onClose).toHaveBeenCalledOnce();
  });

  test("見開きボタンが表示されない", async () => {
    const { loadingTask } = createMockLoadingTask();
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfCgViewer {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-cg-page-area")).toBeTruthy();
    });

    // CgToolbar の showSpread=false により見開きボタンは非表示
    expect(screen.queryByLabelText("見開き切替")).toBeNull();
  });
});
