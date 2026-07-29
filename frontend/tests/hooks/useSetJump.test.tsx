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
import type { JumpListEntry } from "../../src/lib/jumpListNavigation";
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

function createWrapper(initialEntries: string[] = ["/"]) {
  testQueryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  vi.spyOn(testQueryClient, "prefetchInfiniteQuery").mockResolvedValue(undefined);
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={testQueryClient}>
      <MemoryRouter initialEntries={initialEntries}>{children}</MemoryRouter>
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
  useViewerStore.setState({
    viewerOrigin: null,
    viewerTransitionId: 0,
    viewerJumpList: null,
    viewerJumpListIndex: null,
  });
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

function makeJumpEntry(
  id: string,
  kind: JumpListEntry["kind"] = "directory",
  parent: string | null = `parent-of-${id}`,
): JumpListEntry {
  return { node_id: id, parent_node_id: parent, kind, name: id };
}

// jumpList 経路: 任意リスト範囲のセット間ジャンプ
// - viewerJumpList 設定時は FS sibling API を呼ばずリスト内で完結する
// - 境界 (末尾/先頭/未登録 ID/null) と PDF parent null は onBoundary で停止
// - FS sibling フォールバックはしない
describe("useSetJump jumpList 経路", () => {
  test("jumpList 設定時は次のリスト entry に navigate し、sibling API を呼ばない", async () => {
    const list = [makeJumpEntry("current-set"), makeJumpEntry("next-set", "archive")];
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: list,
      viewerJumpListIndex: 0,
    });

    const { result } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSet();
    });
    await waitFor(() => expect(mockNavigate).toHaveBeenCalled());

    // sibling API は呼ばれない (FS sibling フォールバック禁止)
    expect(mockApiFetch).not.toHaveBeenCalled();
    // archive 経路で全ページ fetch
    expect(mockFetchAllBrowsePages).toHaveBeenCalledWith(expect.anything(), "next-set", "name-asc");
  });

  test("navigate 成功後に viewerJumpListIndex が更新される", async () => {
    const list = [
      makeJumpEntry("a", "archive"),
      makeJumpEntry("b", "archive"),
      makeJumpEntry("c", "archive"),
    ];
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: list,
      viewerJumpListIndex: 1,
    });

    const { result } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSet();
    });
    await waitFor(() => expect(mockNavigate).toHaveBeenCalled());

    expect(useViewerStore.getState().viewerJumpListIndex).toBe(2);
  });

  test("Bug 1 回帰: 起動 dirA(0) で着地先が archiveB(1) のまま prev で dirA に戻れる", async () => {
    // 検索結果 [dirA, archiveB, archiveC]、dirA をクリック起動 → first-viewable で archiveB に着地
    // viewer の currentNodeId は archiveB だが viewerJumpListIndex は 0 で固定
    const list: JumpListEntry[] = [
      makeJumpEntry("dirA", "directory"),
      makeJumpEntry("archiveB", "archive"),
      makeJumpEntry("archiveC", "archive"),
    ];
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: list,
      viewerJumpListIndex: 0,
    });

    const onBoundary = vi.fn();
    // viewer は archiveB を表示中（currentNodeId が起動 entry とずれている状態）
    const { result } = renderHook(
      () => useSetJump({ ...defaultProps, currentNodeId: "archiveB", onBoundary }),
      { wrapper: createWrapper() },
    );
    // prev (Z): index=0 → 先頭 boundary（dirA より前は無い、収束ループに陥らない）
    await act(async () => {
      await result.current.goPrevSet();
    });
    expect(onBoundary).toHaveBeenCalledWith("最初のセットです");
    expect(mockNavigate).not.toHaveBeenCalled();
  });

  test("リスト末尾で goNextSet を押すと onBoundary で停止し navigate しない", async () => {
    const list = [makeJumpEntry("only", "archive")];
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: list,
      viewerJumpListIndex: 0,
    });
    const onBoundary = vi.fn();
    const { result } = renderHook(() => useSetJump({ ...defaultProps, onBoundary }), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSet();
    });

    expect(onBoundary).toHaveBeenCalledWith("最後のセットです");
    expect(mockNavigate).not.toHaveBeenCalled();
    expect(mockApiFetch).not.toHaveBeenCalled();
  });

  test("viewerJumpListIndex=null では onBoundary で停止する", async () => {
    const list = [makeJumpEntry("a"), makeJumpEntry("b")];
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: list,
      viewerJumpListIndex: null,
    });
    const onBoundary = vi.fn();
    const { result } = renderHook(() => useSetJump({ ...defaultProps, onBoundary }), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSet();
    });

    expect(onBoundary).toHaveBeenCalled();
    expect(mockNavigate).not.toHaveBeenCalled();
    expect(mockApiFetch).not.toHaveBeenCalled();
  });

  test("PDF target で parent_node_id=null は onBoundary で停止し navigate しない", async () => {
    const list = [makeJumpEntry("current-set"), makeJumpEntry("orphan-pdf", "pdf", null)];
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: list,
      viewerJumpListIndex: 0,
    });
    const onBoundary = vi.fn();
    const { result } = renderHook(() => useSetJump({ ...defaultProps, onBoundary }), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSet();
    });

    expect(onBoundary).toHaveBeenCalledWith("セットを開けません");
    expect(mockNavigate).not.toHaveBeenCalled();
    expect(mockApiFetch).not.toHaveBeenCalled();
  });

  test("Shift+X (goNextSetParent) でも jumpList 経路を優先し sibling API を呼ばない", async () => {
    const list = [makeJumpEntry("current-set"), makeJumpEntry("next-set", "archive")];
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerJumpList: list,
      viewerJumpListIndex: 0,
    });

    const { result } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    await act(async () => {
      await result.current.goNextSetParent();
    });
    await waitFor(() => expect(mockNavigate).toHaveBeenCalled());

    expect(mockApiFetch).not.toHaveBeenCalled();
  });
});

describe("useSetJump unmount キャンセル", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockFetchAllBrowsePages.mockResolvedValue(undefined);
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerTransitionTarget: null,
      viewerJumpList: null,
      viewerJumpListIndex: null,
    });
  });

  test("ビューワーを閉じた (transition cancel) 後の非同期チェーンは navigate しない", async () => {
    // ジャンプのプリフェッチを待っている間に B キーでビューワーを閉じた状況を再現。
    // closeViewer は cancelViewerTransition を呼ぶため、transition id の世代比較で
    // 古いチェーンの navigate が破棄される
    let resolveFetch: (() => void) | null = null;
    mockFetchAllBrowsePages.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveFetch = resolve;
      }),
    );
    mockApiFetch.mockResolvedValue({
      entry: makeEntry({ kind: "archive", node_id: "next-archive" }),
    } satisfies SiblingResponse);

    const { result } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    const jumpPromise = result.current.goNextSetParent();

    // プリフェッチ待ちの間に B キーで閉じる (closeViewer 相当)
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, 10);
      });
      useViewerStore.getState().cancelViewerTransition();
    });
    resolveFetch?.();
    await act(async () => {
      await jumpPromise;
      await new Promise((resolve) => {
        setTimeout(resolve, 20);
      });
    });

    expect(mockNavigate).not.toHaveBeenCalled();
  });

  test("トランジションオーバーレイによる unmount では navigate が完走する", async () => {
    // startViewerTransition の直後に BrowsePage がオーバーレイへ切り替わり
    // ビューワー (と useSetJump) が unmount される。これは意図した遷移中であり、
    // unmount を「閉じた」と誤認して navigate を破棄するとジャンプが不能になる
    mockApiFetch.mockResolvedValue({
      entry: makeEntry({ kind: "archive", node_id: "next-archive" }),
    } satisfies SiblingResponse);
    let resolveFetch: (() => void) | null = null;
    mockFetchAllBrowsePages.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveFetch = resolve;
      }),
    );

    const { result, unmount } = renderHook(() => useSetJump(defaultProps), {
      wrapper: createWrapper(),
    });
    const jumpPromise = result.current.goNextSetParent();

    // startViewerTransition 後のオーバーレイ切替による unmount を再現
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, 10);
      });
    });
    unmount();
    resolveFetch?.();
    await act(async () => {
      await jumpPromise;
      await new Promise((resolve) => {
        setTimeout(resolve, 20);
      });
    });

    expect(mockNavigate).toHaveBeenCalledWith(
      expect.stringContaining("/browse/next-archive"),
      expect.objectContaining({ replace: true }),
    );
  });
});

describe("useSetJump 同親ジャンプ", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockFetchAllBrowsePages.mockResolvedValue(undefined);
    useViewerStore.setState({
      viewerOrigin: null,
      viewerTransitionId: 0,
      viewerTransitionTarget: null,
      viewerJumpList: null,
      viewerJumpListIndex: null,
    });
  });

  test("同親 PDF ジャンプは transition の即時 end に影響されず navigate する", async () => {
    // 遷移先が現在表示中の browse nodeId と同じ (同親 PDF 間ジャンプ) 場合、
    // BrowsePage の data は到着済みで transition が開始直後に end される。
    // これを世代比較が「閉じられた」と誤判定すると同親ジャンプが不能になる
    mockApiFetch.mockResolvedValue({
      entry: makeEntry({ kind: "pdf", node_id: "pdf-next" }),
    } satisfies SiblingResponse);

    // BrowsePage の useBrowseInfiniteData 相当:
    // 遷移先 = 現在 nodeId のため data が既にあり、transition 開始と同時に end される
    const unsubscribe = useViewerStore.subscribe((state) => {
      if (state.viewerTransitionId > 0 && state.viewerTransitionTarget === "parent-1") {
        const tid = state.viewerTransitionId;
        queueMicrotask(() => useViewerStore.getState().endViewerTransition(tid));
      }
    });
    try {
      const { result } = renderHook(() => useSetJump(defaultProps), {
        wrapper: createWrapper(["/browse/parent-1?pdf=pdf-current"]),
      });
      await act(async () => {
        await result.current.goNextSetParent();
        await new Promise((resolve) => {
          setTimeout(resolve, 20);
        });
      });

      expect(mockNavigate).toHaveBeenCalledWith(
        expect.stringMatching(/^\/browse\/parent-1\?.*pdf=pdf-next/),
        expect.objectContaining({ replace: true }),
      );
    } finally {
      unsubscribe();
    }
  });
});
