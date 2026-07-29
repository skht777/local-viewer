// DirectoryTree のクエリキャッシュ共有の検証 (browseQueries はモックしない)
// - 子ノード取得は BrowsePage 本体と同じ browse-infinite キーを共有する
// - hover プリフェッチは limit 付きの browse-infinite を 1 ページだけ温める

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { DirectoryTree } from "../../src/components/DirectoryTree";
import type { BrowseEntry, BrowseResponse } from "../../src/types/api";

Element.prototype.scrollIntoView = vi.fn();

function entry(id: string, kind: BrowseEntry["kind"] = "directory"): BrowseEntry {
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

function browseResponse(nodeId: string, entries: BrowseEntry[]): BrowseResponse {
  return {
    current_node_id: nodeId,
    current_name: nodeId,
    parent_node_id: null,
    ancestors: [],
    entries,
    next_cursor: null,
    total_count: null,
  };
}

function createClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 60_000 } },
  });
}

function renderTree(client: QueryClient, ancestorNodeIds: string[] = []) {
  function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  }
  return render(
    <DirectoryTree
      rootEntries={[entry("photos")]}
      activeNodeId=""
      ancestorNodeIds={ancestorNodeIds}
      onNavigate={vi.fn()}
    />,
    { wrapper: Wrapper },
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("DirectoryTree のキャッシュ共有", () => {
  test("展開時の子ノードは browse-infinite キャッシュから描画される", () => {
    // BrowsePage 本体が読み込んだページをそのまま再利用できる (二重キャッシュの解消)
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    try {
      const client = createClient();
      client.setQueryData(["browse-infinite", "photos", "name-asc"], {
        pages: [browseResponse("photos", [entry("albums"), entry("cover.jpg", "image")])],
        pageParams: [undefined],
      });

      renderTree(client, ["photos"]);

      expect(screen.getByTestId("tree-node-albums")).toBeInTheDocument();
      // 画像はツリーに出さない
      expect(screen.queryByTestId("tree-node-cover.jpg")).not.toBeInTheDocument();
      expect(fetchSpy).not.toHaveBeenCalled();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  test("hover プリフェッチは browse-infinite を 1 ページだけ温める", () => {
    // limit 無しの browse キーで全件スキャンさせない
    vi.useFakeTimers();
    const client = createClient();
    const prefetchSpy = vi.spyOn(client, "prefetchInfiniteQuery").mockResolvedValue(undefined);
    const prefetchQuerySpy = vi.spyOn(client, "prefetchQuery").mockResolvedValue(undefined);

    renderTree(client);

    fireEvent.pointerEnter(screen.getByTestId("tree-node-photos"));
    vi.advanceTimersByTime(200);

    expect(prefetchSpy).toHaveBeenCalledWith(
      expect.objectContaining({ queryKey: ["browse-infinite", "photos", "name-asc"] }),
    );
    expect(prefetchQuerySpy).not.toHaveBeenCalled();
  });
});
