// useSearch フックのテスト
// - デバウンス動作
// - kind フィルタ状態管理
// - 初期状態

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { useSearch } from "../../src/hooks/useSearch";

// API モック — enabled=false のときは queryFn が呼ばれないので安全
vi.mock("../../src/hooks/api/browseQueries", () => ({
  searchOptions: (q: string, kind?: string) => ({
    queryKey: ["search", q, kind],
    queryFn: () => Promise.resolve({ results: [], has_more: false, query: q }),
    enabled: q.length >= 2,
  }),
}));

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe("useSearch", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  test("初期状態で query が空文字", () => {
    const { result } = renderHook(() => useSearch(), { wrapper: createWrapper() });
    expect(result.current.query).toBe("");
    expect(result.current.debouncedQuery).toBe("");
    expect(result.current.results).toEqual([]);
  });

  test("setQuery で query が即座に更新される", () => {
    const { result } = renderHook(() => useSearch(), { wrapper: createWrapper() });
    act(() => result.current.setQuery("hello"));
    expect(result.current.query).toBe("hello");
  });

  test("デバウンス前は debouncedQuery が更新されない", () => {
    const { result } = renderHook(() => useSearch(), { wrapper: createWrapper() });
    act(() => result.current.setQuery("test"));
    expect(result.current.debouncedQuery).toBe("");
  });

  test("300ms 後に debouncedQuery が更新される", async () => {
    const { result } = renderHook(() => useSearch(), { wrapper: createWrapper() });
    act(() => result.current.setQuery("test"));

    await act(async () => {
      vi.advanceTimersByTime(300);
    });

    expect(result.current.debouncedQuery).toBe("test");
  });

  test("kind フィルタを設定・解除できる", () => {
    const { result } = renderHook(() => useSearch(), { wrapper: createWrapper() });
    act(() => result.current.setKind("directory"));
    expect(result.current.kind).toBe("directory");

    act(() => result.current.setKind(null));
    expect(result.current.kind).toBeNull();
  });

  test("連続入力では最後の値のみがデバウンスされる", async () => {
    const { result } = renderHook(() => useSearch(), { wrapper: createWrapper() });

    act(() => result.current.setQuery("a"));
    await act(async () => vi.advanceTimersByTime(100));

    act(() => result.current.setQuery("ab"));
    await act(async () => vi.advanceTimersByTime(100));

    act(() => result.current.setQuery("abc"));
    await act(async () => vi.advanceTimersByTime(300));

    expect(result.current.debouncedQuery).toBe("abc");
  });
});
