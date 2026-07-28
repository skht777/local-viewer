// useEnsureAllBrowsePages の振る舞い検証
// - ビューワー表示中かつ次ページが残っている場合のみ全ページ取得を実行
// - リロード/ディープリンク起動で兄弟画像が 100 件で打ち切られる問題の回帰テスト
// - 取得失敗時も例外を外に漏らさない

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useEnsureAllBrowsePages } from "../../src/hooks/useEnsureAllBrowsePages";

// fetchAllBrowsePages をモック
const mockFetchAllBrowsePages = vi
  .fn<(client: unknown, nodeId: string, sort: string) => Promise<void>>()
  .mockResolvedValue(undefined);
vi.mock("../../src/hooks/api/browseQueries", () => ({
  fetchAllBrowsePages: (...args: [unknown, string, string]) => mockFetchAllBrowsePages(...args),
}));

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

beforeEach(() => {
  mockFetchAllBrowsePages.mockClear();
  mockFetchAllBrowsePages.mockResolvedValue(undefined);
});

describe("useEnsureAllBrowsePages", () => {
  test("ビューワー表示中かつ次ページありで全ページ取得が実行される", async () => {
    renderHook(
      () =>
        useEnsureAllBrowsePages({
          enabled: true,
          nodeId: "node-1",
          sort: "name-asc",
          hasNextPage: true,
        }),
      { wrapper: createWrapper() },
    );
    await waitFor(() => {
      expect(mockFetchAllBrowsePages).toHaveBeenCalledWith(expect.anything(), "node-1", "name-asc");
    });
  });

  test("ビューワーが閉じているときは取得しない", () => {
    renderHook(
      () =>
        useEnsureAllBrowsePages({
          enabled: false,
          nodeId: "node-1",
          sort: "name-asc",
          hasNextPage: true,
        }),
      { wrapper: createWrapper() },
    );
    expect(mockFetchAllBrowsePages).not.toHaveBeenCalled();
  });

  test("次ページがない(全ページ取得済み)ときは取得しない", () => {
    renderHook(
      () =>
        useEnsureAllBrowsePages({
          enabled: true,
          nodeId: "node-1",
          sort: "name-asc",
          hasNextPage: false,
        }),
      { wrapper: createWrapper() },
    );
    expect(mockFetchAllBrowsePages).not.toHaveBeenCalled();
  });

  test("nodeId 未確定のときは取得しない", () => {
    renderHook(
      () =>
        useEnsureAllBrowsePages({
          enabled: true,
          nodeId: undefined,
          sort: "name-asc",
          hasNextPage: true,
        }),
      { wrapper: createWrapper() },
    );
    expect(mockFetchAllBrowsePages).not.toHaveBeenCalled();
  });

  test("1ページ目到着後に hasNextPage が true へ変化した時点で取得が始まる", async () => {
    const { rerender } = renderHook(
      ({ hasNextPage }: { hasNextPage: boolean }) =>
        useEnsureAllBrowsePages({
          enabled: true,
          nodeId: "node-1",
          sort: "name-asc",
          hasNextPage,
        }),
      { wrapper: createWrapper(), initialProps: { hasNextPage: false } },
    );
    expect(mockFetchAllBrowsePages).not.toHaveBeenCalled();

    rerender({ hasNextPage: true });
    await waitFor(() => {
      expect(mockFetchAllBrowsePages).toHaveBeenCalledTimes(1);
    });
  });

  test("取得失敗時も例外を外に漏らさない", async () => {
    mockFetchAllBrowsePages.mockRejectedValue(new Error("network error"));
    renderHook(
      () =>
        useEnsureAllBrowsePages({
          enabled: true,
          nodeId: "node-1",
          sort: "name-asc",
          hasNextPage: true,
        }),
      { wrapper: createWrapper() },
    );
    // unhandled rejection にならず、呼び出し自体は行われている
    await waitFor(() => {
      expect(mockFetchAllBrowsePages).toHaveBeenCalledTimes(1);
    });
  });
});
