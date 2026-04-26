// useBrowseInfiniteData の振る舞い検証
// - useInfiniteQuery の全ページ entries を flatMap し、メタデータは先頭ページから返す
// - data が undefined のとき返り値も undefined
// - viewerTransitionId > 0 + data 到着で endViewerTransition を呼ぶ
// - hasNextPage の boolean 正規化（undefined → false）

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useBrowseInfiniteData } from "../../src/hooks/useBrowseInfiniteData";
import { useViewerStore } from "../../src/stores/viewerStore";
import type { BrowseEntry, BrowseResponse } from "../../src/types/api";

function makeEntry(id: string): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind: "image",
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

function makeResponse(
  overrides: Partial<BrowseResponse> & { entries: BrowseEntry[] },
): BrowseResponse {
  return {
    current_node_id: "n",
    current_name: "n",
    parent_node_id: null,
    ancestors: [],
    next_cursor: null,
    total_count: null,
    ...overrides,
  };
}

function createWrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

function createSeededClient(nodeId: string, sort: string, pages: BrowseResponse[]): QueryClient {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(["browse-infinite", nodeId, sort], {
    pages,
    pageParams: pages.map((_, i) => (i === 0 ? undefined : `cursor-${i}`)),
  });
  return client;
}

beforeEach(() => {
  useViewerStore.setState({ viewerOrigin: null, viewerTransitionId: 0 });
});

describe("useBrowseInfiniteData", () => {
  test("nodeId が undefined のとき data は undefined / isLoading は false", () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useBrowseInfiniteData(undefined, "name-asc"), {
      wrapper: createWrapper(client),
    });
    expect(result.current.data).toBeUndefined();
    expect(result.current.isLoading).toBe(false);
  });

  test("seed したキャッシュから先頭ページのメタ + 結合 entries を返す", () => {
    const page1 = makeResponse({
      current_node_id: "n",
      current_name: "first",
      entries: [makeEntry("a"), makeEntry("b")],
      next_cursor: "cursor-1",
    });
    const page2 = makeResponse({
      current_node_id: "n",
      current_name: "ignored",
      entries: [makeEntry("c")],
    });
    const client = createSeededClient("n", "name-asc", [page1, page2]);
    const { result } = renderHook(() => useBrowseInfiniteData("n", "name-asc"), {
      wrapper: createWrapper(client),
    });
    expect(result.current.data?.current_name).toBe("first");
    expect(result.current.data?.entries.map((e) => e.node_id)).toEqual(["a", "b", "c"]);
  });

  test("hasNextPage が undefined のときも false に正規化される", () => {
    const page = makeResponse({ entries: [makeEntry("a")] });
    const client = createSeededClient("n", "name-asc", [page]);
    const { result } = renderHook(() => useBrowseInfiniteData("n", "name-asc"), {
      wrapper: createWrapper(client),
    });
    expect(result.current.hasNextPage).toBe(false);
  });

  test("data 到着 + viewerTransitionId>0 で endViewerTransition が呼ばれる", async () => {
    const page = makeResponse({ entries: [makeEntry("a")] });
    const client = createSeededClient("n", "name-asc", [page]);

    // transition を開始してから render
    const transitionId = useViewerStore.getState().startViewerTransition();
    expect(useViewerStore.getState().viewerTransitionId).toBe(transitionId);

    renderHook(() => useBrowseInfiniteData("n", "name-asc"), {
      wrapper: createWrapper(client),
    });
    await waitFor(() => {
      expect(useViewerStore.getState().viewerTransitionId).toBe(0);
    });
  });

  test("transition なし（id=0）のとき endViewerTransition は呼ばれない", () => {
    const page = makeResponse({ entries: [makeEntry("a")] });
    const client = createSeededClient("n", "name-asc", [page]);
    expect(useViewerStore.getState().viewerTransitionId).toBe(0);
    renderHook(() => useBrowseInfiniteData("n", "name-asc"), {
      wrapper: createWrapper(client),
    });
    // 引き続き 0 のまま（リセットされない）
    expect(useViewerStore.getState().viewerTransitionId).toBe(0);
  });
});
