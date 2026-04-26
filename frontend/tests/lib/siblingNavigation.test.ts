// siblingNavigation のユニットテスト
// - resolveInitialParent: 純粋関数
// - fetchSiblingPair: /siblings 一括 + fallback の組み合わせを検証
// - fetchSiblingOne: /sibling 単方向 + fallback findNext/Prev を検証
// - walkUpToParent: parentData から { childId, parentId, parentData } の三つ組を返す

import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, test, vi } from "vitest";
import {
  fetchSiblingOne,
  fetchSiblingPair,
  resolveInitialParent,
  walkUpToParent,
} from "../../src/lib/siblingNavigation";
import type {
  AncestorEntry,
  BrowseEntry,
  BrowseResponse,
  SiblingResponse,
  SiblingsResponse,
} from "../../src/types/api";

const mockApiFetch = vi.fn();
vi.mock("../../src/hooks/api/apiClient", () => ({
  apiFetch: (...args: unknown[]) => mockApiFetch(...args),
}));

function makeEntry(kind: BrowseEntry["kind"], id: string): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

function makeBrowseResponse(
  currentNodeId: string,
  parentNodeId: string | null,
  entries: BrowseEntry[],
  ancestors: AncestorEntry[] = [],
): BrowseResponse {
  return {
    current_node_id: currentNodeId,
    current_name: currentNodeId,
    parent_node_id: parentNodeId,
    ancestors,
    entries,
    next_cursor: null,
    total_count: null,
  };
}

function createSeededClient(data: Record<string, BrowseResponse>): QueryClient {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 60_000 } },
  });
  for (const [nodeId, response] of Object.entries(data)) {
    client.setQueryData(["browse", nodeId, "name-asc"], response);
  }
  return client;
}

beforeEach(() => {
  mockApiFetch.mockReset();
});

describe("resolveInitialParent", () => {
  test("parentNodeId が指定されていればそのまま返す", () => {
    const result = resolveInitialParent("parent-1", []);
    expect(result).toBe("parent-1");
  });

  test("parentNodeId が null かつ ancestors[0] があればマウントルートを返す", () => {
    const ancestors: AncestorEntry[] = [{ node_id: "mount", name: "mount" }];
    const result = resolveInitialParent(null, ancestors);
    expect(result).toBe("mount");
  });

  test("parentNodeId が null で ancestors も空なら null", () => {
    const result = resolveInitialParent(null, []);
    expect(result).toBeNull();
  });
});

describe("fetchSiblingPair", () => {
  test("/siblings API で prev+next を取得できれば fallback 不要", async () => {
    const next = makeEntry("directory", "d-next");
    const prev = makeEntry("directory", "d-prev");
    mockApiFetch.mockResolvedValueOnce({ prev, next } satisfies SiblingsResponse);
    const client = new QueryClient();

    const result = await fetchSiblingPair({
      queryClient: client,
      parentId: "parent",
      childId: "child",
      sort: "name-asc",
      needPrev: true,
      needNext: true,
    });

    expect(result).toEqual({ parentData: null, prev, next });
    expect(mockApiFetch).toHaveBeenCalledTimes(1);
  });

  test("/siblings API が失敗したら親ディレクトリ全件で fallback する", async () => {
    mockApiFetch.mockRejectedValueOnce(new Error("api down"));
    const parentData = makeBrowseResponse("parent", null, [
      makeEntry("directory", "a"),
      makeEntry("directory", "child"),
      makeEntry("directory", "z"),
    ]);
    const client = createSeededClient({ parent: parentData });

    const result = await fetchSiblingPair({
      queryClient: client,
      parentId: "parent",
      childId: "child",
      sort: "name-asc",
      needPrev: true,
      needNext: true,
    });

    expect(result?.parentData).toEqual(parentData);
    expect(result?.prev?.node_id).toBe("a");
    expect(result?.next?.node_id).toBe("z");
  });

  test("cancelled() が true なら parentData 取得後に null を返す", async () => {
    mockApiFetch.mockRejectedValueOnce(new Error("api down"));
    const parentData = makeBrowseResponse("parent", null, [makeEntry("directory", "child")]);
    const client = createSeededClient({ parent: parentData });

    const result = await fetchSiblingPair({
      queryClient: client,
      parentId: "parent",
      childId: "child",
      sort: "name-asc",
      needPrev: true,
      needNext: true,
      cancelled: () => true,
    });

    expect(result).toBeNull();
  });
});

describe("fetchSiblingOne", () => {
  test("/sibling API で次セットを取得できれば parentData も返す", async () => {
    const next = makeEntry("directory", "d-next");
    mockApiFetch.mockResolvedValueOnce({ entry: next } satisfies SiblingResponse);
    const parentData = makeBrowseResponse("parent", null, [makeEntry("directory", "child"), next]);
    const client = createSeededClient({ parent: parentData });

    const result = await fetchSiblingOne({
      queryClient: client,
      parentId: "parent",
      childId: "child",
      sort: "name-asc",
      direction: "next",
    });

    expect(result?.sibling).toEqual(next);
    expect(result?.parentData).toEqual(parentData);
  });

  test("/sibling API が失敗時に sortEntries → findNext で fallback する", async () => {
    mockApiFetch.mockRejectedValueOnce(new Error("api down"));
    const parentData = makeBrowseResponse("parent", null, [
      makeEntry("directory", "a"),
      makeEntry("directory", "child"),
      makeEntry("directory", "z"),
    ]);
    const client = createSeededClient({ parent: parentData });

    const result = await fetchSiblingOne({
      queryClient: client,
      parentId: "parent",
      childId: "child",
      sort: "name-asc",
      direction: "next",
    });

    expect(result?.sibling?.node_id).toBe("z");
  });

  test("parentData 取得失敗なら null を返す", async () => {
    mockApiFetch.mockRejectedValueOnce(new Error("api down"));
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    vi.spyOn(client, "fetchQuery").mockRejectedValueOnce(new Error("network"));

    const result = await fetchSiblingOne({
      queryClient: client,
      parentId: "parent",
      childId: "child",
      sort: "name-asc",
      direction: "next",
    });

    expect(result).toBeNull();
  });
});

describe("walkUpToParent", () => {
  test("parentData から { childId, parentId, parentData } を返す", async () => {
    const parentData = makeBrowseResponse("parent", "grand", [], []);
    const client = createSeededClient({ parent: parentData });

    const result = await walkUpToParent({
      queryClient: client,
      currentParentId: "parent",
      sort: "name-asc",
    });

    expect(result).toEqual({ childId: "parent", parentId: "grand", parentData });
  });

  test("parent_node_id が null なら ancestors[0] にフォールバックする", async () => {
    const parentData = makeBrowseResponse(
      "parent",
      null,
      [],
      [{ node_id: "mount", name: "mount" }],
    );
    const client = createSeededClient({ parent: parentData });

    const result = await walkUpToParent({
      queryClient: client,
      currentParentId: "parent",
      sort: "name-asc",
    });

    expect(result?.parentId).toBe("mount");
    expect(result?.childId).toBe("parent");
  });

  test("ancestors も空なら parentId は null", async () => {
    const parentData = makeBrowseResponse("parent", null, [], []);
    const client = createSeededClient({ parent: parentData });

    const result = await walkUpToParent({
      queryClient: client,
      currentParentId: "parent",
      sort: "name-asc",
    });

    expect(result?.parentId).toBeNull();
  });
});
