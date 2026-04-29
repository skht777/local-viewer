// useSearchResultsCallbacks の振る舞い検証
// - handleImageClick: filteredImages → viewerImages 順 index 変換 + setSearchParams + viewerStore.setViewerOrigin
// - handlePdfClick: ?pdf= 付与、index/tab を削除、viewerOrigin 設定
// - handleKindChange: kind フィルタ更新時に viewer 関連パラメータをリセット
// - handleSortChange: relevance なら sort 削除、それ以外は sort 設定
// - handleNavigate: directory/archive クリックで /browse へ遷移

import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { useSearchResultsCallbacks } from "../../src/hooks/useSearchResultsCallbacks";
import { useViewerStore } from "../../src/stores/viewerStore";
import type { BrowseEntry, SearchResult } from "../../src/types/api";

// react-router-dom の useNavigate / useSearchParams をモック化
let lastSetSearchParams: unknown[] = [];
const mockNavigate = vi.fn();
const mockSearchParams = new URLSearchParams("q=foo");
const mockSetSearchParams = vi.fn((arg: unknown, opts?: unknown) => {
  lastSetSearchParams = [arg, opts];
});

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>("react-router-dom");
  return {
    ...actual,
    useNavigate: () => mockNavigate,
    useSearchParams: () => [mockSearchParams, mockSetSearchParams],
  };
});

function makeImage(id: string, name: string): BrowseEntry {
  return {
    node_id: id,
    name,
    kind: "image",
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

function makeSearchEntry(id: string, kind: SearchResult["kind"]): SearchResult {
  return {
    node_id: id,
    parent_node_id: `parent-${id}`,
    name: id,
    kind,
    relative_path: `path/${id}`,
    size_bytes: null,
  };
}

function wrapper({ children }: { children: ReactNode }) {
  return (
    <MemoryRouter initialEntries={["/search?q=foo"]}>
      <Routes>
        <Route path="/search" element={children} />
      </Routes>
    </MemoryRouter>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  lastSetSearchParams = [];
  useViewerStore.setState({
    viewerOrigin: null,
    viewerTransitionId: 0,
    viewerJumpList: null,
    viewerJumpListIndex: null,
  });
});

describe("useSearchResultsCallbacks - handleImageClick", () => {
  test("filteredImages 順 index → viewerImages 順 index に変換し setSearchParams を呼ぶ", () => {
    const filteredImages = [makeImage("i1", "c.jpg"), makeImage("i2", "a.jpg")];
    const viewerIndexMap = new Map([
      ["i2", 0],
      ["i1", 1],
    ]);
    const { result } = renderHook(
      () => useSearchResultsCallbacks({ filteredImages, viewerIndexMap, allRawResults: [] }),
      { wrapper },
    );
    result.current.handleImageClick(0);
    const params = lastSetSearchParams[0] as URLSearchParams;
    expect(params.get("tab")).toBe("images");
    expect(params.get("index")).toBe("1");
    expect(params.has("pdf")).toBe(false);
    expect(params.has("page")).toBe(false);
    // viewerOrigin が設定される
    expect(useViewerStore.getState().viewerOrigin).toEqual({
      pathname: "/search",
      search: "?q=foo",
    });
  });

  test("範囲外の browseIndex では setSearchParams を呼ばない", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleImageClick(99);
    expect(mockSetSearchParams).not.toHaveBeenCalled();
  });
});

describe("useSearchResultsCallbacks - handlePdfClick", () => {
  test("?pdf=<id>&page=1 を設定し index/tab を削除、viewerOrigin を設定する", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handlePdfClick("pdf-1");
    const params = lastSetSearchParams[0] as URLSearchParams;
    expect(params.get("pdf")).toBe("pdf-1");
    expect(params.get("page")).toBe("1");
    expect(params.has("index")).toBe(false);
    expect(params.has("tab")).toBe(false);
    expect(useViewerStore.getState().viewerOrigin).toEqual({
      pathname: "/search",
      search: "?q=foo",
    });
  });
});

describe("useSearchResultsCallbacks - handleKindChange", () => {
  test("newKind が指定されたら kind を設定、viewer 関連を削除", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleKindChange("image");
    const updater = lastSetSearchParams[0] as (prev: URLSearchParams) => URLSearchParams;
    const opts = lastSetSearchParams[1] as { replace: boolean };
    const next = updater(new URLSearchParams("q=foo&index=3&pdf=p&page=2&tab=images"));
    expect(next.get("kind")).toBe("image");
    expect(next.has("index")).toBe(false);
    expect(next.has("pdf")).toBe(false);
    expect(next.has("page")).toBe(false);
    expect(next.has("tab")).toBe(false);
    expect(opts).toEqual({ replace: true });
  });

  test("newKind=null で kind パラメータを削除する", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleKindChange(null);
    const updater = lastSetSearchParams[0] as (prev: URLSearchParams) => URLSearchParams;
    const next = updater(new URLSearchParams("q=foo&kind=image"));
    expect(next.has("kind")).toBe(false);
  });
});

describe("useSearchResultsCallbacks - handleSortChange", () => {
  test("relevance なら sort パラメータを削除", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleSortChange("relevance");
    const updater = lastSetSearchParams[0] as (prev: URLSearchParams) => URLSearchParams;
    const next = updater(new URLSearchParams("q=foo&sort=date-desc"));
    expect(next.has("sort")).toBe(false);
  });

  test("relevance 以外なら sort を設定", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleSortChange("date-desc");
    const updater = lastSetSearchParams[0] as (prev: URLSearchParams) => URLSearchParams;
    const next = updater(new URLSearchParams("q=foo"));
    expect(next.get("sort")).toBe("date-desc");
  });
});

describe("useSearchResultsCallbacks - viewerJumpList", () => {
  test("handlePdfClick で allRawResults の DAP を viewerJumpList に snapshot し index も pdf 位置にセット", () => {
    const allRawResults = [
      makeSearchEntry("dir1", "directory"),
      makeSearchEntry("img1", "image"),
      makeSearchEntry("pdf1", "pdf"),
    ];
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults,
        }),
      { wrapper },
    );
    result.current.handlePdfClick("pdf1");
    const list = useViewerStore.getState().viewerJumpList;
    // image は除外されるため list は [dir1, pdf1]
    expect(list?.map((e) => e.node_id)).toEqual(["dir1", "pdf1"]);
    // pdf1 はフィルタ後 list の index 1（生 allRawResults の index 2 ではない）
    expect(useViewerStore.getState().viewerJumpListIndex).toBe(1);
  });

  test("handlePdfClick で list に該当 pdf が無い場合は (null, null) にフォールバック", () => {
    const allRawResults = [makeSearchEntry("dir1", "directory")];
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults,
        }),
      { wrapper },
    );
    result.current.handlePdfClick("missing-pdf");
    expect(useViewerStore.getState().viewerJumpList).toBeNull();
    expect(useViewerStore.getState().viewerJumpListIndex).toBeNull();
  });

  test("handleImageClick は viewerJumpList を null にクリアする (画像経路は jumpList 対象外)", () => {
    // 事前に jumpList を入れておく → クリアされることを確認
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: [{ node_id: "dir1", parent_node_id: null, kind: "directory", name: "dir1" }],
      viewerJumpListIndex: 0,
    });
    const filteredImages = [makeImage("i1", "a.jpg")];
    const viewerIndexMap = new Map([["i1", 0]]);
    const { result } = renderHook(
      () => useSearchResultsCallbacks({ filteredImages, viewerIndexMap, allRawResults: [] }),
      { wrapper },
    );
    result.current.handleImageClick(0);
    expect(useViewerStore.getState().viewerJumpList).toBeNull();
    expect(useViewerStore.getState().viewerJumpListIndex).toBeNull();
  });
});

describe("useSearchResultsCallbacks - handleNavigate", () => {
  test("tab 未指定で /browse/{id} に navigate する", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleNavigate("d1");
    expect(mockNavigate).toHaveBeenCalledWith("/browse/d1");
  });

  test("tab='videos' なら ?tab=videos が付く", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleNavigate("a1", { tab: "videos" });
    expect(mockNavigate).toHaveBeenCalledWith("/browse/a1?tab=videos");
  });

  test("tab='filesets' は URL に付与しない（既定タブ）", () => {
    const { result } = renderHook(
      () =>
        useSearchResultsCallbacks({
          filteredImages: [],
          viewerIndexMap: new Map(),
          allRawResults: [],
        }),
      { wrapper },
    );
    result.current.handleNavigate("d1", { tab: "filesets" });
    expect(mockNavigate).toHaveBeenCalledWith("/browse/d1");
  });
});
