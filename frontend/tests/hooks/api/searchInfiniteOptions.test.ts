// searchInfiniteOptions のテスト
// - キャッシュキー正規化（trim, scope ?? null, kind ?? "all", sort ?? "relevance"）
// - getNextPageParam が next_offset を返す/null で undefined になる
// - q < 2 文字なら enabled=false

import { describe, expect, test } from "vitest";
import { searchInfiniteOptions } from "../../../src/hooks/api/browseQueries";
import type { SearchResponse } from "../../../src/types/api";

function emptyResponse(nextOffset: number | null): SearchResponse {
  return {
    results: [],
    has_more: nextOffset !== null,
    query: "test",
    next_offset: nextOffset,
  };
}

describe("searchInfiniteOptions", () => {
  test("queryKey が trim/正規化されたキーで構築される", () => {
    const opts = searchInfiniteOptions({ q: "  hello  " });
    expect(opts.queryKey).toEqual(["search-infinite", "hello", null, "all", "relevance"]);
  });

  test("scope/kind/sort 指定が queryKey に反映される", () => {
    const opts = searchInfiniteOptions({
      q: "x",
      scope: "node-1",
      kind: "image",
      sort: "date-desc",
    });
    expect(opts.queryKey).toEqual(["search-infinite", "x", "node-1", "image", "date-desc"]);
  });

  test("getNextPageParam が next_offset を返す", () => {
    const opts = searchInfiniteOptions({ q: "x" });
    // pages/pageParam 引数は無視されるが型互換性のためダミーを渡す
    const next = opts.getNextPageParam(emptyResponse(50), [], 0, []);
    expect(next).toBe(50);
  });

  test("getNextPageParam が next_offset=null で undefined を返す", () => {
    const opts = searchInfiniteOptions({ q: "x" });
    const next = opts.getNextPageParam(emptyResponse(null), [], 0, []);
    expect(next).toBeUndefined();
  });

  test("q が 2 文字未満なら enabled=false", () => {
    const opts = searchInfiniteOptions({ q: "a" });
    expect(opts.enabled).toBe(false);
  });

  test("q を trim 後に 2 文字未満なら enabled=false", () => {
    const opts = searchInfiniteOptions({ q: "  a  " });
    expect(opts.enabled).toBe(false);
  });

  test("q が 2 文字以上なら enabled=true", () => {
    const opts = searchInfiniteOptions({ q: "ab" });
    expect(opts.enabled).toBe(true);
  });
});
