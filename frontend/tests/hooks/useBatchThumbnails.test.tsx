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
  vi.clearAllMocks();
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
    expect(result.current.thumbnails.size).toBe(0);
    expect(result.current.isLoading).toBe(false);
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
      expect(result.current.thumbnails.size).toBe(2);
    });

    expect(result.current.thumbnails.get("node-1")).toMatch(/^blob:/);
    expect(result.current.thumbnails.get("node-2")).toMatch(/^blob:/);
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
      expect(result.current.thumbnails.size).toBe(1);
    });

    expect(result.current.thumbnails.has("node-ok")).toBe(true);
    expect(result.current.thumbnails.has("node-err")).toBe(false);
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
      expect(result.current.thumbnails.size).toBe(1);
    });

    const firstUrl = result.current.thumbnails.get("node-1")!;

    // 新しい node_ids でリレンダー
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-2": { data: btoa("data2") },
      },
    });

    rerender({ ids: ["node-2"] });

    await waitFor(() => {
      expect(result.current.thumbnails.has("node-2")).toBe(true);
    });

    // 前回の URL が revoke されている
    expect(revokedUrls).toContain(firstUrl);
  });

  test("一部 ID 変更時に共通 ID の Blob URL が再利用される", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-1": { data: btoa("data1") },
        "node-2": { data: btoa("data2") },
      },
    });

    const { result, rerender } = renderHook(
      ({ ids }: { ids: string[] }) => useBatchThumbnails(ids),
      { initialProps: { ids: ["node-1", "node-2"] }, wrapper },
    );

    await waitFor(() => {
      expect(result.current.thumbnails.size).toBe(2);
    });

    const url1Before = result.current.thumbnails.get("node-1")!;
    const url2Before = result.current.thumbnails.get("node-2")!;

    // node-2 を残し node-3 を追加
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-2": { data: btoa("data2") },
        "node-3": { data: btoa("data3") },
      },
    });

    rerender({ ids: ["node-2", "node-3"] });

    await waitFor(() => {
      expect(result.current.thumbnails.has("node-3")).toBe(true);
    });

    // node-2 の URL は再利用されている（同じ参照）
    expect(result.current.thumbnails.get("node-2")).toBe(url2Before);
    // node-1 の URL は revoke されている
    expect(revokedUrls).toContain(url1Before);
    // node-2 の URL は revoke されていない
    expect(revokedUrls).not.toContain(url2Before);
  });

  test("チャンクデータ更新時に rawData が再マージされる", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");

    // 1回目: node-1 のみ返す
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
      expect(result.current.thumbnails.size).toBe(1);
    });
    expect(result.current.thumbnails.has("node-1")).toBe(true);

    // 2回目: node-1 を残して node-2 を追加 → 両方返す
    vi.mocked(apiPost).mockResolvedValue({
      thumbnails: {
        "node-1": { data: btoa("data1") },
        "node-2": { data: btoa("data2") },
      },
    });

    rerender({ ids: ["node-1", "node-2"] });

    await waitFor(() => {
      expect(result.current.thumbnails.size).toBe(2);
    });

    // 両方の Blob URL が存在する
    expect(result.current.thumbnails.has("node-1")).toBe(true);
    expect(result.current.thumbnails.has("node-2")).toBe(true);
  });

  test("API エラー時は空マップのまま (フォールバック)", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");
    vi.mocked(apiPost).mockRejectedValue(new Error("network error"));

    const { result } = renderHook(() => useBatchThumbnails(["node-1"]), { wrapper });

    // エラー後も空マップ (クラッシュしない)
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(result.current.thumbnails.size).toBe(0);
  });

  test("バッチ取得中は isLoading が true になる", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");

    let resolveApi!: (value: unknown) => void;
    vi.mocked(apiPost).mockImplementation(
      () => new Promise((resolve) => { resolveApi = resolve; }),
    );

    const { result } = renderHook(() => useBatchThumbnails(["node-1"]), { wrapper });

    // デバウンス待ち
    await act(async () => {
      await new Promise((r) => setTimeout(r, 60));
    });

    // バッチリクエスト中は isLoading = true
    expect(result.current.isLoading).toBe(true);
    expect(result.current.thumbnails.size).toBe(0);

    // レスポンスを返す
    await act(async () => {
      resolveApi({ thumbnails: { "node-1": { data: btoa("data") } } });
    });

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
      expect(result.current.thumbnails.size).toBe(1);
    });
  });

  test("50 件超の node_ids が複数バッチリクエストに分割される", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");

    const ids = Array.from({ length: 60 }, (_, i) => `node-${i}`);

    vi.mocked(apiPost).mockImplementation(async (_url, body) => {
      const reqIds = (body as { node_ids: string[] }).node_ids;
      const thumbnails: Record<string, { data: string }> = {};
      for (const id of reqIds) {
        thumbnails[id] = { data: btoa(`data-${id}`) };
      }
      return { thumbnails };
    });

    const { result } = renderHook(() => useBatchThumbnails(ids), { wrapper });

    await waitFor(() => {
      expect(result.current.thumbnails.size).toBe(60);
    });

    expect(vi.mocked(apiPost)).toHaveBeenCalledTimes(2);

    const firstCallBody = vi.mocked(apiPost).mock.calls[0][1] as { node_ids: string[] };
    expect(firstCallBody.node_ids).toHaveLength(50);

    const secondCallBody = vi.mocked(apiPost).mock.calls[1][1] as { node_ids: string[] };
    expect(secondCallBody.node_ids).toHaveLength(10);
  });

  test("50 件以下は 1 回のバッチリクエストで完結する", async () => {
    const { apiPost } = await import("../../src/hooks/api/apiClient");

    const ids = Array.from({ length: 30 }, (_, i) => `node-${i}`);

    vi.mocked(apiPost).mockImplementation(async (_url, body) => {
      const reqIds = (body as { node_ids: string[] }).node_ids;
      const thumbnails: Record<string, { data: string }> = {};
      for (const id of reqIds) {
        thumbnails[id] = { data: btoa(`data-${id}`) };
      }
      return { thumbnails };
    });

    const { result } = renderHook(() => useBatchThumbnails(ids), { wrapper });

    await waitFor(() => {
      expect(result.current.thumbnails.size).toBe(30);
    });

    expect(vi.mocked(apiPost)).toHaveBeenCalledTimes(1);
    const callBody = vi.mocked(apiPost).mock.calls[0][1] as { node_ids: string[] };
    expect(callBody.node_ids).toHaveLength(30);
  });
});
