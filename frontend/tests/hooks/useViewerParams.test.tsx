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
});
