import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, act } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { useOpenViewerFromEntry } from "../../src/hooks/useOpenViewerFromEntry";
import { useViewerStore } from "../../src/stores/viewerStore";
import type { ResolvedTarget } from "../../src/utils/resolveFirstViewable";
import type { BrowseEntry } from "../../src/types/api";

// resolveFirstViewable をモック
const mockResolveFirstViewable = vi.fn<
  [string, QueryClient, string],
  Promise<ResolvedTarget | null>
>();
vi.mock("../../src/utils/resolveFirstViewable", () => ({
  resolveFirstViewable: (...args: [string, QueryClient, string]) =>
    mockResolveFirstViewable(...args),
}));

// navigate をモック
const mockNavigate = vi.fn();
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual("react-router-dom");
  return { ...actual, useNavigate: () => mockNavigate };
});

// browseInfiniteOptions / fetchAllBrowsePages をモック
const mockFetchAllBrowsePages = vi.fn<[unknown, string, string], Promise<void>>();
vi.mock("../../src/hooks/api/browseQueries", () => ({
  browseInfiniteOptions: (nodeId: string, sort: string) => ({
    queryKey: ["browse-infinite", nodeId, sort],
    queryFn: () => Promise.resolve({ pages: [], pageParams: [] }),
  }),
  fetchAllBrowsePages: (...args: [unknown, string, string]) => mockFetchAllBrowsePages(...args),
}));

// テスト用エントリ生成
function makeEntry(overrides: Partial<BrowseEntry> & { kind: string }): BrowseEntry {
  return {
    node_id: "entry-1",
    name: "test",
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
    ...overrides,
  } as BrowseEntry;
}

// テスト用ラッパー
let testQueryClient: QueryClient = new QueryClient();

function createWrapper() {
  testQueryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  vi.spyOn(testQueryClient, "prefetchInfiniteQuery").mockResolvedValue(undefined);
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={testQueryClient}>
      <MemoryRouter>{children}</MemoryRouter>
    </QueryClientProvider>
  );
}

// デフォルト props
const defaultProps = {
  nodeId: "current-dir",
  mode: "cg" as const,
  sort: "name-asc" as const,
  buildBrowseSearch: (overrides?: { tab?: string; index?: number }) => {
    const sp = new URLSearchParams();
    if (overrides?.tab && overrides.tab !== "filesets") {
      sp.set("tab", overrides.tab);
    }
    if (overrides?.index != null) {
      sp.set("index", String(overrides.index));
    }
    return sp.toString() ? `?${sp}` : "";
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  mockFetchAllBrowsePages.mockResolvedValue(undefined);
  localStorage.clear();
  useViewerStore.setState({
    viewerOrigin: null,
    viewerTransitionId: 0,
  });
});

describe("useOpenViewerFromEntry", () => {
  describe("遷移先の決定", () => {
    test("画像の場合 parentNodeId に navigate する", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "image", node_id: "img-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      // push 遷移: options 未指定（履歴に積んでブラウザバックで呼び出し元に戻れる）
      expect(mockNavigate).toHaveBeenCalledWith(expect.stringContaining("/browse/parent-1"));
    });

    test("PDF の場合 parentNodeId に ?pdf= 付きで navigate する", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "pdf", node_id: "pdf-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockNavigate).toHaveBeenCalledWith(
        expect.stringMatching(/\/browse\/parent-1\?.*pdf=pdf-1/),
      );
    });

    test("アーカイブの場合 entry.node_id に navigate する", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "archive", node_id: "archive-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockNavigate).toHaveBeenCalledWith(expect.stringContaining("/browse/archive-1"));
    });

    test("解決失敗時はディレクトリに navigate する", async () => {
      mockResolveFirstViewable.mockResolvedValue(null);
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockNavigate).toHaveBeenCalledWith("/browse/dir-1");
    });

    test("エラー時はディレクトリに navigate する", async () => {
      mockResolveFirstViewable.mockRejectedValue(new Error("fail"));
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockNavigate).toHaveBeenCalledWith("/browse/dir-1");
    });
  });

  describe("viewerOrigin の設定", () => {
    test("画像を開くときに setViewerOrigin が呼ばれる", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "image", node_id: "img-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      const origin = useViewerStore.getState().viewerOrigin;
      expect(origin).toEqual({ pathname: "/browse/current-dir", search: "" });
    });

    test("PDF を開くときに setViewerOrigin が呼ばれる", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "pdf", node_id: "pdf-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      const origin = useViewerStore.getState().viewerOrigin;
      expect(origin).toEqual({ pathname: "/browse/current-dir", search: "" });
    });

    test("アーカイブを開くときに setViewerOrigin が呼ばれる", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "archive", node_id: "archive-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      const origin = useViewerStore.getState().viewerOrigin;
      expect(origin).toEqual({ pathname: "/browse/current-dir", search: "" });
    });

    test("解決失敗時は viewerOrigin が設定されない", async () => {
      mockResolveFirstViewable.mockResolvedValue(null);
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(useViewerStore.getState().viewerOrigin).toBeNull();
    });
  });

  describe("トランジション制御", () => {
    test("ビューワーを開くときに viewerTransitionId が 0 より大きくなる", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "image", node_id: "img-1" }),
        parentNodeId: "parent-1",
      });
      // startViewerTransition の呼び出しを検出するため、
      // navigate が呼ばれた時点での viewerTransitionId を記録
      let transitionIdAtNavigate = 0;
      mockNavigate.mockImplementation(() => {
        transitionIdAtNavigate = useViewerStore.getState().viewerTransitionId;
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(transitionIdAtNavigate).toBeGreaterThan(0);
    });

    test("解決失敗時は viewerTransitionId が 0 のまま", async () => {
      mockResolveFirstViewable.mockResolvedValue(null);
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(useViewerStore.getState().viewerTransitionId).toBe(0);
    });
  });

  describe("プリフェッチ", () => {
    test("画像を開くときに親ディレクトリの全ページが fetch される", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "image", node_id: "img-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockFetchAllBrowsePages).toHaveBeenCalledWith(
        expect.anything(),
        "parent-1",
        "name-asc",
      );
      expect(testQueryClient.prefetchInfiniteQuery).not.toHaveBeenCalled();
    });

    test("アーカイブを開くときにアーカイブ内の全ページが fetch される", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "archive", node_id: "archive-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockFetchAllBrowsePages).toHaveBeenCalledWith(
        expect.anything(),
        "archive-1",
        "name-asc",
      );
      expect(testQueryClient.prefetchInfiniteQuery).not.toHaveBeenCalled();
    });

    test("PDF を開くときは 1 ページ prefetch のまま（全ページ fetch しない）", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "pdf", node_id: "pdf-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(testQueryClient.prefetchInfiniteQuery).toHaveBeenCalled();
      expect(mockFetchAllBrowsePages).not.toHaveBeenCalled();
    });

    test("fetch が失敗したときはディレクトリに navigate する", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "image", node_id: "img-1" }),
        parentNodeId: "parent-1",
      });
      mockFetchAllBrowsePages.mockRejectedValue(new Error("network error"));
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockNavigate).toHaveBeenCalledWith("/browse/dir-1");
    });
  });

  describe("push モード", () => {
    test("ビューワーを開くときに navigate が push で呼ばれる", async () => {
      mockResolveFirstViewable.mockResolvedValue({
        entry: makeEntry({ kind: "image", node_id: "img-1" }),
        parentNodeId: "parent-1",
      });
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      // push 遷移: options 未指定。ブラウザバックで呼び出し元に戻れることを保証
      expect(mockNavigate).toHaveBeenCalledWith(expect.any(String));
    });

    test("解決失敗時はディレクトリに push で navigate する", async () => {
      mockResolveFirstViewable.mockResolvedValue(null);
      const { result } = renderHook(() => useOpenViewerFromEntry(defaultProps), {
        wrapper: createWrapper(),
      });
      await act(() => result.current("dir-1"));
      expect(mockNavigate).toHaveBeenCalledWith("/browse/dir-1");
    });
  });
});
