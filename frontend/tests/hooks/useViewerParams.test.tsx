import { renderHook, act } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { useViewerParams } from "../../src/hooks/useViewerParams";
import { useViewerStore } from "../../src/stores/viewerStore";
import type { ReactNode } from "react";

function createWrapper(initialEntries: string[] = ["/"]) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <MemoryRouter initialEntries={initialEntries}>{children}</MemoryRouter>;
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

  test("openViewerでindexとtabがURLに設定される", () => {
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

  test("closeViewerでindexがURLから削除される", () => {
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

  // --- closeViewerToOrigin 規約（viewerOrigin フォールバック）---
  //
  // 履歴モデル: 開く系は replace:true で履歴を積まず、閉じる系は viewerOrigin
  // を起点にブラウザ履歴上の browse ページへ戻す（open-viewer-return.test.ts も参照）。
  // 背景: レビュー M1 — closeViewer の分岐は deep link 時の stay-in-place と
  // origin 存在時の restore の 2 経路を明示的に回帰テスト化する。

  test("viewerOriginが設定されていればcloseViewerでorigin側のbrowseURLへ戻る", () => {
    // open 時に setViewerOrigin された状態を再現
    useViewerStore.getState().setViewerOrigin({
      pathname: "/browse/origin-node",
      search: "?mode=manga",
    });

    function LocationProbe({ onChange }: { onChange: (path: string, search: string) => void }) {
      const { useLocation } = require("react-router-dom") as typeof import("react-router-dom");
      const loc = useLocation();
      onChange(loc.pathname, loc.search);
      return null;
    }

    let currentPath = "";
    let currentSearch = "";
    function Wrapper({ children }: { children: ReactNode }) {
      return (
        <MemoryRouter initialEntries={["/browse/viewer-node?tab=images&index=4&mode=manga"]}>
          <Routes>
            <Route
              path="/browse/:nodeId"
              element={
                <>
                  <LocationProbe
                    onChange={(p, s) => {
                      currentPath = p;
                      currentSearch = s;
                    }}
                  />
                  {children}
                </>
              }
            />
          </Routes>
        </MemoryRouter>
      );
    }

    const { result } = renderHook(() => useViewerParams(), { wrapper: Wrapper });
    act(() => {
      result.current.closeViewer();
    });

    // viewerOrigin が消費されていること
    expect(useViewerStore.getState().viewerOrigin).toBeNull();
    // URL が origin 側へ復元されていること
    expect(currentPath).toBe("/browse/origin-node");
    expect(currentSearch).toBe("?mode=manga");
  });

  test("viewerOriginがnullならcloseViewerはindexだけを削除し現在位置に留まる", () => {
    // 明示的に origin を null に初期化（前テストの持ち越しを避ける）
    useViewerStore.getState().setViewerOrigin(null);

    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/browse/current-node?tab=images&index=4&mode=manga"]),
    });
    act(() => {
      result.current.closeViewer();
    });

    // 現在ディレクトリに留まる: mode/tab は維持し index のみ消える
    expect(result.current.params.index).toBe(-1);
    expect(result.current.params.mode).toBe("manga");
    expect(result.current.params.tab).toBe("images");
    expect(useViewerStore.getState().viewerOrigin).toBeNull();
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

  test("openPdfViewerでpdfとpageがURLに設定される", () => {
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

  // --- mode 正規化 ---

  test("setMode('manga')でURLにmode=mangaが設定される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    act(() => {
      result.current.setMode("manga");
    });
    expect(result.current.params.mode).toBe("manga");
  });

  test("setMode('cg')でURLからmodeが削除される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?mode=manga"]),
    });
    expect(result.current.params.mode).toBe("manga");
    act(() => {
      result.current.setMode("cg");
    });
    expect(result.current.params.mode).toBe("cg");
  });

  test("不正なmode値はcgに正規化される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?mode=invalid"]),
    });
    expect(result.current.params.mode).toBe("cg");
  });

  // --- 排他制御 ---

  test("pdfとindexが同時に存在する場合pdfが優先される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?pdf=pdf123&index=3&page=2&mode=cg&tab=images"]),
    });
    expect(result.current.isPdfViewerOpen).toBe(true);
    expect(result.current.isViewerOpen).toBe(false);
  });

  // --- sort パラメータ ---

  test("デフォルトのsortがname-ascである", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    expect(result.current.params.sort).toBe("name-asc");
  });

  test("URLのsort=date-descが反映される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?sort=date-desc"]),
    });
    expect(result.current.params.sort).toBe("date-desc");
  });

  test("setSortでURLが更新される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    act(() => {
      result.current.setSort("date-desc");
    });
    expect(result.current.params.sort).toBe("date-desc");
  });

  test("setSort('name-asc')でURLからsortが削除される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?sort=date-desc"]),
    });
    act(() => {
      result.current.setSort("name-asc");
    });
    expect(result.current.params.sort).toBe("name-asc");
  });

  test("buildBrowseSearchでsort=date-descが引き継がれる", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?sort=date-desc"]),
    });
    const search = result.current.buildBrowseSearch();
    expect(search).toContain("sort=date-desc");
  });

  test("不正なsort値はname-ascに正規化される", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?sort=invalid"]),
    });
    expect(result.current.params.sort).toBe("name-asc");
  });

  // --- buildBrowseSearch index オプション ---

  test("buildBrowseSearchでindex指定時にURLにindexが含まれる", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(["/?mode=manga"]),
    });
    const search = result.current.buildBrowseSearch({ tab: "images", index: 0 });
    expect(search).toContain("index=0");
    expect(search).toContain("tab=images");
  });

  test("buildBrowseSearchでindex未指定時にURLにindexが含まれない", () => {
    const { result } = renderHook(() => useViewerParams(), {
      wrapper: createWrapper(),
    });
    const search = result.current.buildBrowseSearch();
    expect(search).not.toContain("index");
  });
});
