import { renderHook, act } from "@testing-library/react";
import { useFullscreen } from "../../src/hooks/useFullscreen";

describe("useFullscreen", () => {
  let mockRequestFullscreen: ReturnType<typeof vi.fn> = vi.fn();
  let mockExitFullscreen: ReturnType<typeof vi.fn> = vi.fn();

  beforeEach(() => {
    // jsdom は Fullscreen API 未実装のためモック
    mockRequestFullscreen = vi.fn().mockResolvedValue(undefined);
    mockExitFullscreen = vi.fn().mockResolvedValue(undefined);
    document.documentElement.requestFullscreen =
      mockRequestFullscreen as unknown as typeof document.documentElement.requestFullscreen;
    document.exitFullscreen = mockExitFullscreen as unknown as typeof document.exitFullscreen;
    Object.defineProperty(document, "fullscreenElement", {
      value: null,
      writable: true,
      configurable: true,
    });
  });

  test("初期状態で isFullscreen が false", () => {
    const { result } = renderHook(() => useFullscreen());
    expect(result.current.isFullscreen).toBe(false);
  });

  test("toggleFullscreen が requestFullscreen を呼ぶ", () => {
    const { result } = renderHook(() => useFullscreen());
    act(() => {
      result.current.toggleFullscreen();
    });
    expect(mockRequestFullscreen).toHaveBeenCalledOnce();
  });

  test("フルスクリーン中に toggleFullscreen が exitFullscreen を呼ぶ", () => {
    // フルスクリーン状態をシミュレート
    Object.defineProperty(document, "fullscreenElement", {
      value: document.documentElement,
      writable: true,
      configurable: true,
    });

    const { result } = renderHook(() => useFullscreen());
    act(() => {
      result.current.toggleFullscreen();
    });
    expect(mockExitFullscreen).toHaveBeenCalledOnce();
  });
});
