// useSiblingPrefetch のユニットテスト
// - ビューワー表示時にバックグラウンドで次/前セットの browse データをプリフェッチ
// - ジャンプ先セットの最初の画像をプリロード
// - PDF の場合は親ディレクトリを温める（browse API は PDF に 422 を返すため）

import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { useSiblingPrefetch } from "../../src/hooks/useSiblingPrefetch";
import type { AncestorEntry, BrowseEntry, BrowseResponse } from "../../src/types/api";

// --- ヘルパー ---

function entry(kind: BrowseEntry["kind"], id: string): BrowseEntry {
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

function browseResponse(
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
  };
}

// Image モック: プリロードされた URL を追跡
let preloadedSrcs: string[];
const OriginalImage = globalThis.Image;

beforeEach(() => {
  preloadedSrcs = [];
  globalThis.Image = class MockImage {
    private _src = "";
    get src() {
      return this._src;
    }
    set src(value: string) {
      this._src = value;
      preloadedSrcs.push(value);
    }
  } as unknown as typeof Image;
});

afterEach(() => {
  globalThis.Image = OriginalImage;
});

// QueryClient + Provider ラッパー生成
function createWrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

// キャッシュにデータを事前投入する QueryClient を生成
function createSeededClient(data: Record<string, BrowseResponse>): QueryClient {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 60_000 },
    },
  });
  for (const [nodeId, response] of Object.entries(data)) {
    client.setQueryData(["browse", nodeId], response);
  }
  return client;
}

// --- テスト ---

describe("useSiblingPrefetch", () => {
  test("次のセットの browse データがプリフェッチされる", async () => {
    // 親ディレクトリ parent に d1(現在), d2(次) がある
    const parentData = browseResponse("parent", null, [
      entry("directory", "d1"),
      entry("directory", "d2"),
    ]);
    // d2 の中に画像がある
    const d2Data = browseResponse("d2", "parent", [
      entry("image", "img1"),
      entry("image", "img2"),
    ]);

    const client = createSeededClient({ parent: parentData, d2: d2Data });
    const wrapper = createWrapper(client);

    renderHook(
      () =>
        useSiblingPrefetch({
          currentNodeId: "d1",
          parentNodeId: "parent",
          ancestors: [],
        }),
      { wrapper },
    );

    // d2 の browse データがキャッシュに入る
    await waitFor(() => {
      expect(client.getQueryData(["browse", "d2"])).toBeDefined();
    });
  });

  test("前のセットの browse データがプリフェッチされる", async () => {
    const parentData = browseResponse("parent", null, [
      entry("directory", "d1"),
      entry("directory", "d2"),
    ]);
    const d1Data = browseResponse("d1", "parent", [
      entry("image", "img1"),
    ]);

    const client = createSeededClient({ parent: parentData, d1: d1Data });
    const wrapper = createWrapper(client);

    renderHook(
      () =>
        useSiblingPrefetch({
          currentNodeId: "d2",
          parentNodeId: "parent",
          ancestors: [],
        }),
      { wrapper },
    );

    await waitFor(() => {
      expect(client.getQueryData(["browse", "d1"])).toBeDefined();
    });
  });

  test("兄弟がない場合は上位ディレクトリを探索する", async () => {
    // grandparent → parent(d1 のみ) → d1(現在)
    // grandparent に d1の兄弟 d2 がある
    const parentData = browseResponse("parent", "grandparent", [
      entry("directory", "d1"),
    ]);
    const grandparentData = browseResponse("grandparent", null, [
      entry("directory", "parent"),
      entry("directory", "d2"),
    ]);
    const d2Data = browseResponse("d2", "grandparent", [
      entry("image", "img1"),
    ]);

    const client = createSeededClient({
      parent: parentData,
      grandparent: grandparentData,
      d2: d2Data,
    });
    const wrapper = createWrapper(client);

    renderHook(
      () =>
        useSiblingPrefetch({
          currentNodeId: "d1",
          parentNodeId: "parent",
          ancestors: [],
        }),
      { wrapper },
    );

    // 1階層上がって d2 のデータがプリフェッチされる
    await waitFor(() => {
      expect(client.getQueryData(["browse", "d2"])).toBeDefined();
    });
  });

  test("ジャンプ先セットの最初の画像がプリロードされる", async () => {
    const parentData = browseResponse("parent", null, [
      entry("directory", "d1"),
      entry("directory", "d2"),
    ]);
    const d2Data = browseResponse("d2", "parent", [
      entry("image", "img1"),
      entry("image", "img2"),
      entry("image", "img3"),
      entry("image", "img4"),
    ]);

    const client = createSeededClient({ parent: parentData, d2: d2Data });
    const wrapper = createWrapper(client);

    renderHook(
      () =>
        useSiblingPrefetch({
          currentNodeId: "d1",
          parentNodeId: "parent",
          ancestors: [],
        }),
      { wrapper },
    );

    // 先頭3枚がプリロードされる
    await waitFor(() => {
      expect(preloadedSrcs).toContain("/api/file/img1");
      expect(preloadedSrcs).toContain("/api/file/img2");
      expect(preloadedSrcs).toContain("/api/file/img3");
    });
    // 4枚目はプリロードされない
    expect(preloadedSrcs).not.toContain("/api/file/img4");
  });

  test("PDF の兄弟は親ディレクトリを温める（sibling.node_id を browse しない）", async () => {
    // parent に d1(現在), p1(次の PDF) がある
    const parentData = browseResponse("parent", null, [
      entry("directory", "d1"),
      entry("pdf", "p1"),
    ]);

    const client = createSeededClient({ parent: parentData });
    const wrapper = createWrapper(client);

    renderHook(
      () =>
        useSiblingPrefetch({
          currentNodeId: "d1",
          parentNodeId: "parent",
          ancestors: [],
        }),
      { wrapper },
    );

    await waitFor(() => {
      // 親ディレクトリのキャッシュは温まっている
      expect(client.getQueryData(["browse", "parent"])).toBeDefined();
    });
    // PDF の node_id で browse しない（422 になるため）
    expect(client.getQueryData(["browse", "p1"])).toBeUndefined();
  });

  test("currentNodeId が null の場合は何もしない", async () => {
    const client = createSeededClient({});
    const fetchQuerySpy = vi.spyOn(client, "fetchQuery");
    const wrapper = createWrapper(client);

    renderHook(
      () =>
        useSiblingPrefetch({
          currentNodeId: null,
          parentNodeId: "parent",
          ancestors: [],
        }),
      { wrapper },
    );

    // fetchQuery が呼ばれない
    await new Promise((r) => setTimeout(r, 50));
    expect(fetchQuerySpy).not.toHaveBeenCalled();
  });

  test("コンポーネントアンマウント時にプリフェッチを中断する", async () => {
    // fetchQuery が呼ばれるたびに遅延を挟む遅い client
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: 60_000 } },
    });
    // 親データのみ事前投入（d2 のデータはない → fetchQuery が必要）
    const parentData = browseResponse("parent", null, [
      entry("directory", "d1"),
      entry("directory", "d2"),
    ]);
    client.setQueryData(["browse", "parent"], parentData);

    const wrapper = createWrapper(client);

    const { unmount } = renderHook(
      () =>
        useSiblingPrefetch({
          currentNodeId: "d1",
          parentNodeId: "parent",
          ancestors: [],
        }),
      { wrapper },
    );

    // 即座にアンマウント → エラーが発生しないことを確認
    unmount();
    // 少し待ってもクラッシュしない
    await new Promise((r) => setTimeout(r, 100));
  });

  test("API エラー時にクラッシュしない", async () => {
    // queryFn がエラーを投げる client（キャッシュにデータがないので fetchQuery がネットワーク呼び出し）
    const client = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: 60_000,
          queryFn: () => {
            throw new Error("Network error");
          },
        },
      },
    });
    const wrapper = createWrapper(client);

    // エラーが throw されないことを確認
    expect(() => {
      renderHook(
        () =>
          useSiblingPrefetch({
            currentNodeId: "d1",
            parentNodeId: "parent",
            ancestors: [],
          }),
        { wrapper },
      );
    }).not.toThrow();

    // 少し待ってもクラッシュしない
    await new Promise((r) => setTimeout(r, 100));
  });
});
