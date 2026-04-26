// usePdfRenderCache フックのテスト

import { renderHook, act } from "@testing-library/react";
import { vi, describe, test, expect, beforeEach } from "vitest";
import { usePdfRenderCache } from "../../src/hooks/usePdfRenderCache";

// ImageBitmap のモック
function createMockBitmap(width = 100, height = 100): ImageBitmap {
  return {
    width,
    height,
    close: vi.fn(),
  } as unknown as ImageBitmap;
}

describe("usePdfRenderCache", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("キャッシュミスでundefinedを返す", () => {
    const { result } = renderHook(() => usePdfRenderCache());
    expect(result.current.get("1:1.5")).toBeUndefined();
  });

  test("キャッシュヒットでImageBitmapを返す", () => {
    const { result } = renderHook(() => usePdfRenderCache());
    const bitmap = createMockBitmap();

    act(() => result.current.put("1:1.5", bitmap));
    expect(result.current.get("1:1.5")).toBe(bitmap);
  });

  test("invalidateで全エントリがクリアされる", () => {
    const { result } = renderHook(() => usePdfRenderCache());
    const b1 = createMockBitmap();
    const b2 = createMockBitmap();

    act(() => {
      result.current.put("1:1.0", b1);
      result.current.put("2:1.0", b2);
    });

    act(() => result.current.invalidate());

    expect(result.current.get("1:1.0")).toBeUndefined();
    expect(result.current.get("2:1.0")).toBeUndefined();
    expect(b1.close).toHaveBeenCalledOnce();
    expect(b2.close).toHaveBeenCalledOnce();
  });

  test("maxBytes超過でLRU追い出しされる", () => {
    // maxBytes = 100*100*4 * 2 = 80000 (2エントリ分)
    const maxBytes = 100 * 100 * 4 * 2;
    const { result } = renderHook(() => usePdfRenderCache(maxBytes));

    const b1 = createMockBitmap(100, 100);
    const b2 = createMockBitmap(100, 100);
    const b3 = createMockBitmap(100, 100);

    act(() => {
      result.current.put("1:1.0", b1);
      result.current.put("2:1.0", b2);
    });

    // b1, b2 で上限ちょうど。b3 を追加すると b1 (LRU) が追い出される
    act(() => result.current.put("3:1.0", b3));

    expect(result.current.get("1:1.0")).toBeUndefined();
    expect(b1.close).toHaveBeenCalledOnce();
    expect(result.current.get("2:1.0")).toBe(b2);
    expect(result.current.get("3:1.0")).toBe(b3);
  });

  test("同じキーで上書きすると古い bitmap が close される", () => {
    const { result } = renderHook(() => usePdfRenderCache());
    const b1 = createMockBitmap();
    const b2 = createMockBitmap();

    act(() => result.current.put("1:1.0", b1));
    act(() => result.current.put("1:1.0", b2));

    expect(b1.close).toHaveBeenCalledOnce();
    expect(result.current.get("1:1.0")).toBe(b2);
  });

  test("maxBytesを超える単一エントリはキャッシュされない", () => {
    // 非常に小さい
    const maxBytes = 100;
    const { result } = renderHook(() => usePdfRenderCache(maxBytes));
    // 100*100*4 = 40000 > 100
    const bitmap = createMockBitmap(100, 100);

    act(() => result.current.put("1:1.0", bitmap));

    expect(result.current.get("1:1.0")).toBeUndefined();
    expect(bitmap.close).toHaveBeenCalledOnce();
  });
});
