// useSetJump のプリフェッチ分岐のテスト
// - PDF セットジャンプは 1 ページ prefetch のみ
// - image / archive セットジャンプは fetchAllBrowsePages で全ページ取得
// - directory→image 解決時に fetchAllBrowsePages を await してから navigate

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { useSetJump } from "../../src/hooks/useSetJump";
import { useViewerStore } from "../../src/stores/viewerStore";
import type { ResolvedTarget } from "../../src/utils/resolveFirstViewable";
import type { BrowseEntry, SiblingResponse } from "../../src/types/api";

// resolveFirstViewable をモック
const mockResolveFirstViewable =
  vi.fn<
    (nodeId: string, queryClient: QueryClient, sort: string) => Promise<ResolvedTarget | null>
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

// browseQueries をモック
const mockFetchAllBrowsePages =
  vi.fn<(client: unknown, nodeId: string, sort: string) => Promise<void>>();
vi.mock("../../src/hooks/api/browseQueries", () => ({
  browseInfiniteOptions: (nodeId: string, sort: string) => ({
    queryKey: ["browse-infinite", nodeId, sort],
    queryFn: () => Promise.resolve({ pages: [], pageParams: [] }),
  }),
  browseNodeOptions: (nodeId: string, sort: string) => ({
    queryKey: ["browse", nodeId, sort],
    queryFn: () =>
      Promise.resolve({
        current_node_id: nodeId,
        current_name: nodeId,
        parent_node_id: null,
        ancestors: [],
        entries: [],
        next_cursor: null,
        total_count: null,
      }),
  }),
  fetchAllBrowsePages: (...args: [unknown, string, string]) => mockFetchAllBrowsePages(...args),
}));

// sibling API をモック
const mockApiFetch = vi.fn();
vi.mock("../../src/hooks/api/apiClient", () => ({
  apiFetch: (...args: unknown[]) => mockApiFetch(...args),
}));

// テスト用エントリ
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

const defaultProps = {
  currentNodeId: "current-set",
  parentNodeId: "parent-1",
  ancestors: [],
  mode: "cg" as const,
  sort: "name-asc" as const,
};

beforeEach(() => {
  vi.clearAllMocks();
  mockFetchAllBrowsePages.mockResolvedValue(undefined);
  // viewerTransitionId は zustand のグローバル状態。テスト間でリセットしないと
  // 前テストの startViewerTransition が次の goNextSetParent の早期 return を引き起こす
  useViewerStore.setState({ viewerOrigin: null, viewerTransitionId: 0 });
});

describe("useSetJump プリフェッチ分岐", () => {
  test("PDF セットジャンプは 1 ページ prefetch のみで全ページ fetch しない", async () => {
    // sibling API が PDF エントリを返す
    mockApiFetch.mockResolvedValue({
      entry: makeEntry({ kind: "pdf", node_id: "pdf-next" }),
    } satisfies SiblingResponse);

    const { result } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSetParent();
    });
    // navigateToTarget は fire-and-forget なので navigate を待つ
    await waitFor(() => expect(mockNavigate).toHaveBeenCalled());

    // PDF: 全ページ fetch は呼ばれず、1 ページ prefetch のみ
    expect(mockFetchAllBrowsePages).not.toHaveBeenCalled();
    expect(testQueryClient.prefetchInfiniteQuery).toHaveBeenCalled();
  });

  test("archive セットジャンプは fetchAllBrowsePages で全ページ取得する", async () => {
    mockApiFetch.mockResolvedValue({
      entry: makeEntry({ kind: "archive", node_id: "archive-next" }),
    } satisfies SiblingResponse);

    const { result } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSetParent();
    });
    // navigateToTarget は fire-and-forget なので mockNavigate が呼ばれるまで待つ
    await waitFor(() => expect(mockNavigate).toHaveBeenCalled());

    expect(mockFetchAllBrowsePages).toHaveBeenCalledWith(
      expect.anything(),
      "archive-next",
      "name-asc",
    );
  });

  test("directory→image 解決時に親ディレクトリの全ページが fetch される", async () => {
    // sibling API がディレクトリを返し、resolveFirstViewable で image に解決
    mockApiFetch.mockResolvedValue({
      entry: makeEntry({ kind: "directory", node_id: "dir-next" }),
    } satisfies SiblingResponse);
    mockResolveFirstViewable.mockResolvedValue({
      entry: makeEntry({ kind: "image", node_id: "img-1" }),
      parentNodeId: "img-parent",
    });

    const { result } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSetParent();
    });
    await waitFor(() => expect(mockNavigate).toHaveBeenCalled());

    // image branch: fetchAllBrowsePages が parent に対して呼ばれること
    expect(mockFetchAllBrowsePages).toHaveBeenCalledWith(
      expect.anything(),
      "img-parent",
      "name-asc",
    );
  });
});
