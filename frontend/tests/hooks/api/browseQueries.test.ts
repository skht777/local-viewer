// fetchAllBrowsePages ヘルパーのテスト
// - next_cursor が null になるまで fetch を継続することを検証
// - MAX_PAGES 到達時に警告ログを出して中断することを検証

import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { MAX_PAGES, fetchAllBrowsePages } from "../../../src/hooks/api/browseQueries";
import type { BrowseResponse } from "../../../src/types/api";

function makeResponse(
  entries: BrowseResponse["entries"],
  nextCursor: string | null,
): BrowseResponse {
  return {
    current_node_id: "node-1",
    current_name: "dir",
    parent_node_id: null,
    ancestors: [],
    entries,
    next_cursor: nextCursor,
    total_count: null,
  };
}

function installFetchMock(
  sequence: BrowseResponse[] | ((call: number) => BrowseResponse),
): () => number {
  let call = 0;
  global.fetch = vi.fn(async () => {
    const resp = Array.isArray(sequence)
      ? sequence[Math.min(call, sequence.length - 1)]
      : sequence(call);
    call++;
    return {
      ok: true,
      json: async () => resp,
    } as Response;
  }) as unknown as typeof fetch;
  return () => call;
}

describe("fetchAllBrowsePages", () => {
  let queryClient: QueryClient;
  const nodeId = "node-1";
  const sort = "name-asc";

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  test("next_cursor が null になるまで fetch を継続する", async () => {
    const getCallCount = installFetchMock([
      makeResponse([], "cursor-1"),
      makeResponse([], "cursor-2"),
      makeResponse([], null),
    ]);

    await fetchAllBrowsePages(queryClient, nodeId, sort);

    const cached = queryClient.getQueryData<{ pages: BrowseResponse[] }>([
      "browse-infinite",
      nodeId,
      sort,
    ]);
    expect(cached?.pages).toHaveLength(3);
    expect(getCallCount()).toBe(3);
  });

  test("fresh な 1 ページ目キャッシュがあっても残りページを取得する", async () => {
    // App.tsx 既定の staleTime: 5 分を再現
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: 60_000 } },
    });
    const getCallCount = installFetchMock([
      makeResponse([], "cursor-1"),
      makeResponse([], "cursor-2"),
      makeResponse([], null),
    ]);

    // useSiblingPrefetch が先に 1 ページ目だけ prefetch しているケース
    await queryClient.prefetchInfiniteQuery({
      queryKey: ["browse-infinite", nodeId, sort],
      queryFn: async () => ({
        current_node_id: nodeId,
        current_name: "dir",
        parent_node_id: null,
        ancestors: [],
        entries: [],
        next_cursor: "cursor-1",
        total_count: null,
      }),
      initialPageParam: undefined,
      getNextPageParam: () => undefined,
    });
    expect(getCallCount()).toBe(0); // prefetch はモック fetch を経由しない

    await fetchAllBrowsePages(queryClient, nodeId, sort);

    const cached = queryClient.getQueryData<{ pages: BrowseResponse[] }>([
      "browse-infinite",
      nodeId,
      sort,
    ]);
    // staleTime 内 fresh な 1 ページキャッシュがあっても、最終ページまで揃う
    expect(cached?.pages).toHaveLength(3);
    expect(cached?.pages.at(-1)?.next_cursor).toBeNull();
  });

  test("全ページ取得済みキャッシュなら追加 fetch しない", async () => {
    // MAX_PAGES に達するなどで完了済みのキャッシュは再 fetch 不要
    const getCallCount = installFetchMock([makeResponse([], null)]);

    // 完了済みキャッシュを直接投入
    queryClient.setQueryData(["browse-infinite", nodeId, sort], {
      pages: [makeResponse([], null)],
      pageParams: [undefined],
    });

    await fetchAllBrowsePages(queryClient, nodeId, sort);

    expect(getCallCount()).toBe(0); // 追加 fetch 0 件
  });

  test("MAX_PAGES に達したら警告ログを出して中断する", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const getCallCount = installFetchMock((i) => makeResponse([], `cursor-${i + 1}`));

    await fetchAllBrowsePages(queryClient, nodeId, sort);

    const cached = queryClient.getQueryData<{ pages: BrowseResponse[] }>([
      "browse-infinite",
      nodeId,
      sort,
    ]);
    expect(cached?.pages).toHaveLength(MAX_PAGES);
    // off-by-one 検出: サーバー呼び出し回数と取得ページ数が一致すること
    expect(getCallCount()).toBe(MAX_PAGES);
    expect(warnSpy).toHaveBeenCalled();
  });
});
