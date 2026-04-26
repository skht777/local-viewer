// PdfMangaViewer コンポーネントのテスト

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi, describe, test, expect, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
}));

// PdfCanvas をモックして enableTextLayer の受け渡しを検証
const mockPdfCanvasProps: Record<string, unknown>[] = [];
vi.mock("../../src/components/PdfCanvas", () => ({
  PdfCanvas: (props: Record<string, unknown>) => {
    mockPdfCanvasProps.push({ ...props });
    return null;
  },
}));

// jsdom では要素の高さが 0 のため virtualizer がアイテムを生成しない
// useVirtualizer をモックして仮想アイテムを強制的に返す
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 800,
    getVirtualItems: () =>
      Array.from({ length: Math.min(count, 5) }, (_, i) => ({
        index: i,
        start: i * 800,
        size: 800,
        end: (i + 1) * 800,
        key: i,
      })),
    measureElement: () => {},
    measure: () => {},
    scrollToIndex: () => {},
  }),
}));

import { getDocument } from "../../src/lib/pdfjs";
import { PdfMangaViewer } from "../../src/components/PdfMangaViewer";

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
  return { loadingTask, mockDocument };
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

function defaultProps(overrides: Partial<React.ComponentProps<typeof PdfMangaViewer>> = {}) {
  return {
    pdfNodeId: "pdf123",
    pdfName: "test.pdf",
    parentNodeId: "dir456",
    initialPage: 1,
    mode: "manga" as const,
    onPageChange: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
}

describe("PdfMangaViewer", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockPdfCanvasProps.length = 0;
    HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
      clearRect: vi.fn(),
      drawImage: vi.fn(),
    })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    Element.prototype.scrollIntoView = vi.fn();
  });

  test("PDF読み込み中にローディング表示", () => {
    const loadingTask = { promise: new Promise(() => {}), destroy: vi.fn() };
    mockGetDocument.mockReturnValue(loadingTask as unknown as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfMangaViewer {...defaultProps()} />);

    expect(screen.getByTestId("pdf-loading")).toBeTruthy();
  });

  test("読み込み完了後にスクロールエリアが表示される", async () => {
    const { loadingTask } = createMockLoadingTask(3);
    mockGetDocument.mockReturnValue(loadingTask as unknown as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfMangaViewer {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-manga-scroll-area")).toBeTruthy();
    });
  });

  test("読み込みエラー時にエラーメッセージを表示", async () => {
    const loadingTask = createErrorLoadingTask("Broken PDF");
    mockGetDocument.mockReturnValue(loadingTask as unknown as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfMangaViewer {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-error")).toBeTruthy();
    });
    expect(screen.getByText(/Broken PDF/)).toBeTruthy();
  });

  test("読み込み完了後にテキストレイヤーが有効化されている", async () => {
    const { loadingTask } = createMockLoadingTask(3);
    mockGetDocument.mockReturnValue(loadingTask as unknown as ReturnType<typeof getDocument>);

    renderWithProviders(<PdfMangaViewer {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-manga-scroll-area")).toBeTruthy();
    });

    // PdfCanvas が enableTextLayer={true} で呼び出されていること
    await waitFor(() => {
      expect(mockPdfCanvasProps.length).toBeGreaterThan(0);
    });
    const lastProps = mockPdfCanvasProps.at(-1);
    expect(lastProps?.enableTextLayer).toBe(true);
  });

  test("閉じるボタンでonCloseが呼ばれる", async () => {
    const { loadingTask } = createMockLoadingTask();
    mockGetDocument.mockReturnValue(loadingTask as unknown as ReturnType<typeof getDocument>);
    const onClose = vi.fn();

    renderWithProviders(<PdfMangaViewer {...defaultProps({ onClose })} />);

    await waitFor(() => {
      expect(screen.getByTestId("pdf-manga-scroll-area")).toBeTruthy();
    });

    await userEvent.click(screen.getByLabelText("閉じる"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
