// useBatchThumbnails フックのテスト
// - base64 → Blob URL 変換
// - node_ids 変更時に Blob URL が更新される
// - 空 node_ids で空マップが返る
// - revokeObjectURL によるクリーンアップ

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { useBatchThumbnails } from "../../src/hooks/api/thumbnailQueries";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

// apiPost をモック
vi.mock("../../src/hooks/api/apiClient", () => ({
  apiPost: vi.fn(),
  ApiError: class ApiError extends Error {
    status: number;
    constructor(status: number, message: string) {
      super(message);
      this.status = status;
    }
  },
}));

// URL.createObjectURL / revokeObjectURL をモック
const createdUrls: string[] = [];
const revokedUrls: string[] = [];
let urlCounter = 0;

beforeEach(() => {
  queryClient.clear();
  createdUrls.length = 0;
  revokedUrls.length = 0;
  urlCounter = 0;

  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn(() => {
      const url = `blob:mock-${++urlCounter}`;
      createdUrls.push(url);
      return url;
    }),
    revokeObjectURL: vi.fn((url: string) => {
      revokedUrls.push(url);
    }),
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("useBatchThumbnails", () => {
  test("空の node_ids で空マップを返す", () => {
    const { result } = renderHook(() => useBatchThumbnails([]), { wrapper });
    expect(result.current.size).toBe(0);
  });

  test("node_ids に対して Blob URL マップが返る", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-1": { data: btoa("fake-jpeg-data") },
        "node-2": { data: btoa("another-jpeg") },
      },
    });

    const { result } = renderHook(() => useBatchThumbnails(["node-1", "node-2"]), { wrapper });

    await waitFor(() => {
      expect(result.current.size).toBe(2);
    });

    expect(result.current.get("node-1")).toMatch(/^blob:/);
    expect(result.current.get("node-2")).toMatch(/^blob:/);
  });

  test("エラーレスポンスのエントリは Blob URL に含まれない", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-ok": { data: btoa("ok") },
        "node-err": { error: "not found", code: "NOT_FOUND" },
      },
    });

    const { result } = renderHook(() => useBatchThumbnails(["node-ok", "node-err"]), { wrapper });

    await waitFor(() => {
      expect(result.current.size).toBe(1);
    });

    expect(result.current.has("node-ok")).toBe(true);
    expect(result.current.has("node-err")).toBe(false);
  });

  test("node_ids 変更時に前回の Blob URL が revoke される", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-1": { data: btoa("data1") },
      },
    });

    const { result, rerender } = renderHook(
      ({ ids }: { ids: string[] }) => useBatchThumbnails(ids),
      { initialProps: { ids: ["node-1"] }, wrapper },
    );

    await waitFor(() => {
      expect(result.current.size).toBe(1);
    });

    const firstUrl = result.current.get("node-1")!;

    // 新しい node_ids でリレンダー
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-2": { data: btoa("data2") },
      },
    });

    rerender({ ids: ["node-2"] });

    await waitFor(() => {
      expect(result.current.has("node-2")).toBe(true);
    });

    // 前回の URL が revoke されている
    expect(revokedUrls).toContain(firstUrl);
  });

  test("API エラー時は空マップのまま (フォールバック)", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");
    vi.mocked(apiPost).mockRejectedValue(new Error("network error"));

    const { result } = renderHook(() => useBatchThumbnails(["node-1"]), { wrapper });

    // エラー後も空マップ (クラッシュしない)
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(result.current.size).toBe(0);
  });
});
