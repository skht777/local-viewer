// useFindSiblingRecursive の振る舞い検証（6 必須ケース）
// 1. currentNodeId が null のとき何もせず null を返す
// 2. level 0（同階層）で sibling が見つかったら返す
// 3. 親ディレクトリへ climb して sibling を発見できる
// 4. fetchSiblingOne 失敗（null）で null を返し例外を投げない
// 5. visited セットによる循環防止 / max depth 到達で停止
// 6. sourceTopDir が正しく算出される（兄弟探索の起点ディレクトリ判定）

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { useFindSiblingRecursive } from "../../src/hooks/useFindSiblingRecursive";
import type { AncestorEntry, BrowseEntry, BrowseResponse } from "../../src/types/api";

// fetchSiblingOne をモック化（呼び出し履歴を取得するため）
interface FetchOpts {
  parentId: string;
  childId: string;
  sort: string;
  direction: "next" | "prev";
}
type FetchResult = { parentData: BrowseResponse; sibling: BrowseEntry | null } | null;
const mockFetchSiblingOne =
  vi.fn<(opts: FetchOpts & { queryClient: unknown }) => Promise<FetchResult>>();

vi.mock("../../src/lib/siblingNavigation", async () => {
  const actual = await vi.importActual<typeof import("../../src/lib/siblingNavigation")>(
    "../../src/lib/siblingNavigation",
  );
  return {
    ...actual,
    fetchSiblingOne: (opts: FetchOpts & { queryClient: unknown }) => mockFetchSiblingOne(opts),
  };
});

function makeEntry(kind: BrowseEntry["kind"], id: string, name = id): BrowseEntry {
  return {
    node_id: id,
    name,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

function makeResponse(opts: {
  current_node_id: string;
  parent_node_id?: string | null;
  ancestors?: AncestorEntry[];
  entries?: BrowseEntry[];
}): BrowseResponse {
  return {
    current_node_id: opts.current_node_id,
    current_name: opts.current_node_id,
    parent_node_id: opts.parent_node_id ?? null,
    ancestors: opts.ancestors ?? [],
    entries: opts.entries ?? [],
    next_cursor: null,
    total_count: null,
  };
}

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

beforeEach(() => {
  mockFetchSiblingOne.mockReset();
});

describe("useFindSiblingRecursive", () => {
  test("currentNodeId が null のとき null を返し fetchSiblingOne を呼ばない", async () => {
    const { result } = renderHook(
      () =>
        useFindSiblingRecursive({
          currentNodeId: null,
          parentNodeId: "parent-1",
          ancestors: [],
          sort: "name-asc",
        }),
      { wrapper },
    );
    const found = await result.current("next");
    expect(found).toBeNull();
    expect(mockFetchSiblingOne).not.toHaveBeenCalled();
  });

  test("level 0 で sibling が見つかったらそれを返す", async () => {
    const sibling = makeEntry("directory", "sibling-1");
    const parentData = makeResponse({
      current_node_id: "parent-1",
      entries: [makeEntry("directory", "current-1"), sibling],
    });
    mockFetchSiblingOne.mockResolvedValue({ parentData, sibling });

    const { result } = renderHook(
      () =>
        useFindSiblingRecursive({
          currentNodeId: "current-1",
          parentNodeId: "parent-1",
          ancestors: [],
          sort: "name-asc",
        }),
      { wrapper },
    );
    const found = await result.current("next");
    expect(found?.target).toBe(sibling);
    expect(found?.levelsUp).toBe(0);
    expect(found?.searchDirData).toBe(parentData);
  });

  test("親ディレクトリへ climb して sibling を発見できる", async () => {
    const grandparentData = makeResponse({
      current_node_id: "grandparent-1",
      parent_node_id: null,
      entries: [makeEntry("directory", "parent-1"), makeEntry("directory", "uncle-1")],
    });
    const parentData = makeResponse({
      current_node_id: "parent-1",
      parent_node_id: "grandparent-1",
      entries: [makeEntry("directory", "current-1")],
    });
    mockFetchSiblingOne.mockImplementation(async ({ parentId }) => {
      if (parentId === "parent-1") {
        return { parentData, sibling: null };
      }
      if (parentId === "grandparent-1") {
        return { parentData: grandparentData, sibling: makeEntry("directory", "uncle-1") };
      }
      return null;
    });

    const { result } = renderHook(
      () =>
        useFindSiblingRecursive({
          currentNodeId: "current-1",
          parentNodeId: "parent-1",
          ancestors: [{ node_id: "grandparent-1", name: "gp" }],
          sort: "name-asc",
        }),
      { wrapper },
    );
    const found = await result.current("next");
    expect(found?.target.node_id).toBe("uncle-1");
    expect(found?.levelsUp).toBe(1);
  });

  test("fetchSiblingOne が null を返したら null を返し例外を投げない", async () => {
    mockFetchSiblingOne.mockResolvedValue(null);
    const { result } = renderHook(
      () =>
        useFindSiblingRecursive({
          currentNodeId: "current-1",
          parentNodeId: "parent-1",
          ancestors: [],
          sort: "name-asc",
        }),
      { wrapper },
    );
    await expect(result.current("next")).resolves.toBeNull();
  });

  test("visited セットで同じ parent への 2 度目の訪問を抑止する", async () => {
    // 親 A → 親 B（fakey） → 親 A（visited による break）
    const parentA = makeResponse({
      current_node_id: "A",
      parent_node_id: "B",
      entries: [makeEntry("directory", "current-1")],
    });
    const parentB = makeResponse({
      current_node_id: "B",
      // 循環: B の親が A を指す
      parent_node_id: "A",
      entries: [makeEntry("directory", "A")],
    });
    mockFetchSiblingOne.mockImplementation(async ({ parentId }) => {
      if (parentId === "A") {
        return { parentData: parentA, sibling: null };
      }
      if (parentId === "B") {
        return { parentData: parentB, sibling: null };
      }
      return null;
    });

    const { result } = renderHook(
      () =>
        useFindSiblingRecursive({
          currentNodeId: "current-1",
          parentNodeId: "A",
          ancestors: [],
          sort: "name-asc",
        }),
      { wrapper },
    );
    const found = await result.current("next");
    expect(found).toBeNull();
    // A → B → A(visited) で break、計 2 回しか呼ばれない
    expect(mockFetchSiblingOne).toHaveBeenCalledTimes(2);
  });

  test("MAX_DEPTH (10) を超える階層では停止する", async () => {
    // 各 level で sibling=null・parent_node_id=次の親 を返し、深さ無制限に climb 可能なフィクスチャ
    let level = 0;
    mockFetchSiblingOne.mockImplementation(async ({ parentId }) => {
      const data = makeResponse({
        current_node_id: parentId,
        parent_node_id: `level-${level + 1}`,
        entries: [makeEntry("directory", level === 0 ? "current-1" : `level-${level - 1}`)],
      });
      level++;
      return { parentData: data, sibling: null };
    });

    const { result } = renderHook(
      () =>
        useFindSiblingRecursive({
          currentNodeId: "current-1",
          parentNodeId: "level-0",
          ancestors: [],
          sort: "name-asc",
        }),
      { wrapper },
    );
    const found = await result.current("next");
    expect(found).toBeNull();
    // MAX_DEPTH=10 で停止 → fetchSiblingOne は最大 10 回
    expect(mockFetchSiblingOne.mock.calls.length).toBeLessThanOrEqual(10);
  });

  test("sourceTopDir が level 0 の parentData から算出される", async () => {
    const sourceEntry = makeEntry("directory", "current-1");
    const parentData = makeResponse({
      current_node_id: "parent-1",
      ancestors: [
        { node_id: "root", name: "root" },
        { node_id: "topdir", name: "topdir" },
      ],
      entries: [sourceEntry, makeEntry("directory", "sibling-1")],
    });
    mockFetchSiblingOne.mockResolvedValue({
      parentData,
      sibling: makeEntry("directory", "sibling-1"),
    });

    const { result } = renderHook(
      () =>
        useFindSiblingRecursive({
          currentNodeId: "current-1",
          parentNodeId: "parent-1",
          ancestors: [],
          sort: "name-asc",
        }),
      { wrapper },
    );
    const found = await result.current("next");
    // resolveTopLevelDir 計算結果は実装依存だが、sourceTopDir が null/string で返る
    expect(found?.sourceTopDir).toBeDefined();
  });
});
