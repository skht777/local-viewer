import { renderHook, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { useViewerParams } from "../../src/hooks/useViewerParams";
import type { ReactNode } from "react";

function createWrapper(initialEntries: string[] = ["/"]) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <MemoryRouter initialEntries={initialEntries}>{children}</MemoryRouter>
    );
  };
}

describe("useViewerParams", () => {
  test("デフォルト値が返される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    expect(result.current.params.tab).toBe("filesets");
    // index パラメータ未設定時は -1（ビューワー未開始）
    expect(result.current.params.index).toBe(-1);
    expect(result.current.params.mode).toBe("cg");
  });

  test("URLのsearchParamsが反映される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=videos&index=5&mode=manga"]),
    });
    expect(result.current.params.tab).toBe("videos");
    expect(result.current.params.index).toBe(5);
    expect(result.current.params.mode).toBe("manga");
  });

  test("setTabでURLが更新される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    act(() => {
      result.current.setTab("videos");
    });
    expect(result.current.params.tab).toBe("videos");
  });

  // --- Phase 2: ビューワー開閉ヘルパー ---

  test("indexパラメータなしでisViewerOpenがfalse", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=images"]),
    });
    expect(result.current.isViewerOpen).toBe(false);
  });

  test("tab=images かつ index ありで isViewerOpen が true", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=images&index=3&mode=cg"]),
    });
    expect(result.current.isViewerOpen).toBe(true);
  });

  test("tab=videos かつ index ありでも isViewerOpen が false", () => {
    // ビューワーは images タブでのみ有効
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=videos&index=3&mode=cg"]),
    });
    expect(result.current.isViewerOpen).toBe(false);
  });

  test("openViewerでindex・mode・tabがURLに設定される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=filesets"]),
    });
    act(() => {
      result.current.openViewer(5);
    });
    expect(result.current.params.tab).toBe("images");
    expect(result.current.params.index).toBe(5);
    expect(result.current.params.mode).toBe("cg");
    expect(result.current.isViewerOpen).toBe(true);
  });

  test("closeViewerでindexとmodeがURLから削除される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=images&index=3&mode=cg"]),
    });
    act(() => {
      result.current.closeViewer();
    });
    expect(result.current.isViewerOpen).toBe(false);
    // index パラメータが削除されていること
    expect(result.current.params.index).toBe(-1);
  });

  // --- Phase 6: PDF ビューワー状態 ---

  test("pdfパラメータがない場合isPdfViewerOpenはfalse", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=filesets"]),
    });
    expect(result.current.isPdfViewerOpen).toBe(false);
    expect(result.current.params.pdfNodeId).toBeNull();
    expect(result.current.params.pdfPage).toBe(1);
  });

  test("openPdfViewerでpdfとpageとmodeがURLに設定される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=filesets"]),
    });
    act(() => {
      result.current.openPdfViewer("pdf123");
    });
    expect(result.current.isPdfViewerOpen).toBe(true);
    expect(result.current.params.pdfNodeId).toBe("pdf123");
    expect(result.current.params.pdfPage).toBe(1);
    expect(result.current.params.mode).toBe("cg");
  });

  test("openPdfViewerでindex/tabが削除される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?tab=images&index=5&mode=cg"]),
    });
    act(() => {
      result.current.openPdfViewer("pdf123");
    });
    expect(result.current.isPdfViewerOpen).toBe(true);
    expect(result.current.isViewerOpen).toBe(false);
    expect(result.current.params.index).toBe(-1);
  });

  test("openViewerでpdf/pageが削除される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?pdf=pdf123&page=5&mode=cg"]),
    });
    act(() => {
      result.current.openViewer(3);
    });
    expect(result.current.isViewerOpen).toBe(true);
    expect(result.current.isPdfViewerOpen).toBe(false);
    expect(result.current.params.pdfNodeId).toBeNull();
  });

  test("closePdfViewerでpdfとpageが削除される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?pdf=pdf123&page=3&mode=cg"]),
    });
    act(() => {
      result.current.closePdfViewer();
    });
    expect(result.current.isPdfViewerOpen).toBe(false);
    expect(result.current.params.pdfNodeId).toBeNull();
  });

  test("setPdfPageでpageが更新される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?pdf=pdf123&page=1&mode=cg"]),
    });
    act(() => {
      result.current.setPdfPage(5);
    });
    expect(result.current.params.pdfPage).toBe(5);
  });

  test("pdfとindexが同時に存在する場合pdfが優先される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?pdf=pdf123&index=3&page=2&mode=cg&tab=images"]),
    });
    expect(result.current.isPdfViewerOpen).toBe(true);
    expect(result.current.isViewerOpen).toBe(false);
  });
});
