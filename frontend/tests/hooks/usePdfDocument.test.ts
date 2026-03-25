// usePdfDocument フックのテスト
// - src/lib/pdfjs のみをモック (pdfjs-dist を直接 import しない)
// - loading task + document の二段階ライフサイクル管理を検証

import { renderHook, act, waitFor } from "@testing-library/react";
import { vi, describe, test, expect, beforeEach } from "vitest";

// src/lib/pdfjs をモック
vi.mock("../../src/lib/pdfjs", () => {
  return {
    getDocument: vi.fn(),
  };
});

import { getDocument } from "../../src/lib/pdfjs";
import { usePdfDocument } from "../../src/hooks/usePdfDocument";

const mockGetDocument = vi.mocked(getDocument);

// テスト用のモックファクトリ
function createMockLoadingTask(pageCount = 5) {
  const mockDocument = {
    numPages: pageCount,
    destroy: vi.fn(),
    getPage: vi.fn(),
  };

  let resolvePromise: (doc: typeof mockDocument) => void;
  let rejectPromise: (err: Error) => void;
  const promise = new Promise<typeof mockDocument>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });

  const loadingTask = {
    promise,
    destroy: vi.fn(),
  };

  return {
    loadingTask,
    mockDocument,
    resolve: () => resolvePromise(mockDocument),
    reject: (err: Error) => rejectPromise(err),
  };
}

describe("usePdfDocument", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("読み込み中はisLoading=trueを返す", () => {
    const { loadingTask } = createMockLoadingTask();
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    const { result } = renderHook(() => usePdfDocument("/api/file/abc123"));

    expect(result.current.isLoading).toBe(true);
    expect(result.current.document).toBeNull();
    expect(result.current.pageCount).toBe(0);
    expect(result.current.error).toBeNull();
  });

  test("読み込み完了後にpageCountを返す", async () => {
    const { loadingTask, mockDocument, resolve } = createMockLoadingTask(10);
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    const { result } = renderHook(() => usePdfDocument("/api/file/abc123"));

    await act(async () => {
      resolve();
    });

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });
    expect(result.current.document).toBe(mockDocument);
    expect(result.current.pageCount).toBe(10);
    expect(result.current.error).toBeNull();
  });

  test("アンマウント時にloadingTaskをdestroyする", async () => {
    const { loadingTask } = createMockLoadingTask();
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    const { unmount } = renderHook(() => usePdfDocument("/api/file/abc123"));

    unmount();

    expect(loadingTask.destroy).toHaveBeenCalledOnce();
  });

  test("読み込み完了後のアンマウントでdocumentもdestroyする", async () => {
    const { loadingTask, mockDocument, resolve } = createMockLoadingTask();
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    const { result, unmount } = renderHook(() => usePdfDocument("/api/file/abc123"));

    await act(async () => {
      resolve();
    });
    await waitFor(() => {
      expect(result.current.document).toBe(mockDocument);
    });

    unmount();

    expect(loadingTask.destroy).toHaveBeenCalled();
    expect(mockDocument.destroy).toHaveBeenCalledOnce();
  });

  test("URL変更時に旧loadingTaskをdestroyして再読み込みする", async () => {
    const first = createMockLoadingTask(5);
    const second = createMockLoadingTask(8);

    mockGetDocument
      .mockReturnValueOnce(first.loadingTask as ReturnType<typeof getDocument>)
      .mockReturnValueOnce(second.loadingTask as ReturnType<typeof getDocument>);

    const { result, rerender } = renderHook(
      ({ url }) => usePdfDocument(url),
      { initialProps: { url: "/api/file/aaa" } },
    );

    // 最初の読み込み完了
    await act(async () => {
      first.resolve();
    });
    await waitFor(() => {
      expect(result.current.pageCount).toBe(5);
    });

    // URL 変更
    rerender({ url: "/api/file/bbb" });

    // 旧 loading task と document が破棄される
    expect(first.loadingTask.destroy).toHaveBeenCalled();
    expect(first.mockDocument.destroy).toHaveBeenCalledOnce();

    // 新しい読み込みが開始
    expect(result.current.isLoading).toBe(true);

    // 新しい読み込み完了
    await act(async () => {
      second.resolve();
    });
    await waitFor(() => {
      expect(result.current.pageCount).toBe(8);
    });
  });

  test("読み込みエラー時にerrorを返す", async () => {
    const { loadingTask, reject } = createMockLoadingTask();
    mockGetDocument.mockReturnValue(loadingTask as ReturnType<typeof getDocument>);

    const { result } = renderHook(() => usePdfDocument("/api/file/abc123"));

    await act(async () => {
      reject(new Error("Invalid PDF"));
    });

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });
    expect(result.current.error).toBeInstanceOf(Error);
    expect(result.current.error?.message).toBe("Invalid PDF");
    expect(result.current.document).toBeNull();
  });

  test("URL変更中にレスポンスが来た場合は古い結果を破棄する", async () => {
    const first = createMockLoadingTask(5);
    const second = createMockLoadingTask(8);

    mockGetDocument
      .mockReturnValueOnce(first.loadingTask as ReturnType<typeof getDocument>)
      .mockReturnValueOnce(second.loadingTask as ReturnType<typeof getDocument>);

    const { result, rerender } = renderHook(
      ({ url }) => usePdfDocument(url),
      { initialProps: { url: "/api/file/aaa" } },
    );

    // URL 変更（最初の読み込み完了前）
    rerender({ url: "/api/file/bbb" });

    // 古い読み込みが遅れて完了 → document は state にセットされない
    await act(async () => {
      first.resolve();
    });

    // 新しい読み込み完了
    await act(async () => {
      second.resolve();
    });

    await waitFor(() => {
      expect(result.current.pageCount).toBe(8);
    });
    // 古い document は destroy されたはず
    expect(first.mockDocument.destroy).toHaveBeenCalledOnce();
  });
});
