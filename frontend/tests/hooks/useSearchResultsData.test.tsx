// useSearchResultsData のユニットテスト
// - URL searchParams から q / scope / kind / sort を正規化
// - searchInfiniteOptions の data を BrowseEntry[] に変換
// - 不正な kind / sort はデフォルトに正規化される

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { useSearchResultsData } from "../../src/hooks/useSearchResultsData";
import type { SearchResponse, SearchResult } from "../../src/types/api";

const mockApiFetch = vi.fn();
vi.mock("../../src/hooks/api/apiClient", () => ({
  apiFetch: (...args: unknown[]) => mockApiFetch(...args),
}));

function makeSearchResult(id: string, kind: SearchResult["kind"]): SearchResult {
  return {
    node_id: id,
    parent_node_id: null,
    name: id,
    kind,
    relative_path: id,
    size_bytes: null,
  };
}

function emptyResponse(): SearchResponse {
  return { results: [], has_more: false, query: "", next_offset: null };
}

function createWrapper(initialEntry: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={[initialEntry]}>
          <Routes>
            <Route path="/search" element={children} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    );
  };
}

beforeEach(() => {
  mockApiFetch.mockReset();
  // browseNodeOptions の scope 取得は空応答 (scopeName=null になる)
  mockApiFetch.mockResolvedValue(emptyResponse());
});

describe("useSearchResultsData", () => {
  test("URL searchParams から q/scope/kind/sort を正規化する", () => {
    const wrapper = createWrapper("/search?q=abc&scope=node-1&kind=image&sort=name-asc");
    const { result } = renderHook(() => useSearchResultsData(), { wrapper });

    expect(result.current.q).toBe("abc");
    expect(result.current.scope).toBe("node-1");
    expect(result.current.kind).toBe("image");
    expect(result.current.sort).toBe("name-asc");
  });

  test("不正な kind / sort はデフォルトへフォールバックする", () => {
    const wrapper = createWrapper("/search?q=abc&kind=foo&sort=bar");
    const { result } = renderHook(() => useSearchResultsData(), { wrapper });

    expect(result.current.kind).toBeNull();
    expect(result.current.sort).toBe("relevance");
  });

  test("q が空白を含む場合は trim される", () => {
    const wrapper = createWrapper("/search?q=%20%20hello%20%20");
    const { result } = renderHook(() => useSearchResultsData(), { wrapper });

    expect(result.current.q).toBe("hello");
  });

  test("検索結果が SearchResult[] → BrowseEntry[] に変換される", async () => {
    const searchResp: SearchResponse = {
      results: [makeSearchResult("img-1", "image"), makeSearchResult("dir-1", "directory")],
      has_more: false,
      query: "abc",
      next_offset: null,
    };
    mockApiFetch.mockReset();
    mockApiFetch.mockImplementation(async (path: string) => {
      if (path.includes("/api/search")) {
        return searchResp;
      }
      return emptyResponse();
    });

    const wrapper = createWrapper("/search?q=abc");
    const { result } = renderHook(() => useSearchResultsData(), { wrapper });

    await waitFor(() => {
      expect(result.current.allEntries).toHaveLength(2);
    });
    expect(result.current.allEntries[0].node_id).toBe("img-1");
    expect(result.current.allEntries[0].kind).toBe("image");
    expect(result.current.allEntries[1].kind).toBe("directory");
  });

  test("q が 2 文字未満なら検索クエリは無効で allEntries は空配列", async () => {
    const wrapper = createWrapper("/search?q=a");
    const { result } = renderHook(() => useSearchResultsData(), { wrapper });

    expect(result.current.allEntries).toEqual([]);
    expect(result.current.isLoading).toBe(false);
  });
});
