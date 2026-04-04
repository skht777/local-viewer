import { QueryClient } from "@tanstack/react-query";
import { resolveFirstViewable } from "../../src/utils/resolveFirstViewable";
import type { BrowseEntry, BrowseResponse } from "../../src/types/api";

function entry(
  kind: BrowseEntry["kind"],
  id: string,
  overrides?: Partial<BrowseEntry>,
): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
    ...overrides,
  };
}

function browseResponse(nodeId: string, entries: BrowseEntry[]): BrowseResponse {
  return {
    current_node_id: nodeId,
    current_name: nodeId,
    parent_node_id: null,
    ancestors: [],
    entries,
  };
}

// fetchQuery をモックして、nodeId に応じた BrowseResponse を返す
function createMockQueryClient(responses: Record<string, BrowseResponse>): QueryClient {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  vi.spyOn(client, "fetchQuery").mockImplementation(async (opts) => {
    const key = opts.queryKey as string[];
    const nodeId = key[1];
    const response = responses[nodeId];
    if (!response) throw new Error(`No mock for nodeId: ${nodeId}`);
    return response;
  });
  return client;
}

describe("resolveFirstViewable", () => {
  test("直下に画像があるディレクトリでは画像と親を返す", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [entry("image", "img1"), entry("image", "img2")]),
    });

    const result = await resolveFirstViewable("dir1", qc);
    expect(result).toEqual({ entry: entry("image", "img1"), parentNodeId: "dir1" });
  });

  test("直下にアーカイブがあればアーカイブを優先して返す", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [
        entry("image", "img1"),
        entry("archive", "arc1"),
        entry("directory", "sub1"),
      ]),
    });

    const result = await resolveFirstViewable("dir1", qc);
    expect(result?.entry.node_id).toBe("arc1");
    expect(result?.entry.kind).toBe("archive");
  });

  test("直下にPDFがあればPDFを返す", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [entry("pdf", "pdf1")]),
    });

    const result = await resolveFirstViewable("dir1", qc);
    expect(result?.entry.node_id).toBe("pdf1");
    expect(result?.entry.kind).toBe("pdf");
  });

  test("サブディレクトリのみの場合は再帰降下して最初の閲覧対象を返す", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [entry("directory", "sub1"), entry("directory", "sub2")]),
      sub1: browseResponse("sub1", [entry("image", "img1")]),
    });

    const result = await resolveFirstViewable("dir1", qc);
    expect(result?.entry.node_id).toBe("img1");
    expect(result?.parentNodeId).toBe("sub1");
  });

  test("2階層ネストのディレクトリを再帰降下する", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [entry("directory", "sub1")]),
      sub1: browseResponse("sub1", [entry("directory", "sub2")]),
      sub2: browseResponse("sub2", [entry("archive", "arc1")]),
    });

    const result = await resolveFirstViewable("dir1", qc);
    expect(result?.entry.node_id).toBe("arc1");
    expect(result?.parentNodeId).toBe("sub2");
  });

  test("空のディレクトリではnullを返す", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", []),
    });

    const result = await resolveFirstViewable("dir1", qc);
    expect(result).toBeNull();
  });

  test("閲覧対象がないディレクトリ（videoのみ）ではnullを返す", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [entry("video", "v1")]),
    });

    const result = await resolveFirstViewable("dir1", qc);
    expect(result).toBeNull();
  });

  test("date-descソートで最新のアーカイブが選択される", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [
        entry("archive", "old", { name: "old", modified_at: 1000 }),
        entry("archive", "new", { name: "new", modified_at: 2000 }),
      ]),
    });

    const result = await resolveFirstViewable("dir1", qc, "date-desc");
    expect(result?.entry.node_id).toBe("new");
  });

  test("name-descソートで名前降順の最初のエントリが選択される", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [
        entry("image", "alpha", { name: "alpha" }),
        entry("image", "zeta", { name: "zeta" }),
      ]),
    });

    const result = await resolveFirstViewable("dir1", qc, "name-desc");
    expect(result?.entry.node_id).toBe("zeta");
  });

  test("ソート順がデフォルト(name-asc)の場合は元の順序を維持する", async () => {
    const qc = createMockQueryClient({
      dir1: browseResponse("dir1", [
        entry("image", "alpha", { name: "alpha" }),
        entry("image", "zeta", { name: "zeta" }),
      ]),
    });

    const result = await resolveFirstViewable("dir1", qc, "name-asc");
    expect(result?.entry.node_id).toBe("alpha");
  });
});
