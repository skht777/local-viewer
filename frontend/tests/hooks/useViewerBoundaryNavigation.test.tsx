// useViewerBoundaryNavigation の振る舞い検証
// - canGoNext=false で handleGoNext を呼ぶと toast 表示 + nav.goNext は呼ばれない
// - canGoPrev=false で handleGoPrev を呼ぶと toast 表示 + nav.goPrev は呼ばれない
// - 境界以外では nav.goNext/goPrev が呼ばれ toast は呼ばれない
// - firstMessage/lastMessage の上書き

import { renderHook } from "@testing-library/react";
import { useViewerBoundaryNavigation } from "../../src/hooks/useViewerBoundaryNavigation";

interface SetupOpts {
  canGoNext: boolean;
  canGoPrev: boolean;
  firstMessage?: string;
  lastMessage?: string;
}

function setup(opts: SetupOpts) {
  const goNext = vi.fn();
  const goPrev = vi.fn();
  const showToast = vi.fn();
  const { result } = renderHook(() =>
    useViewerBoundaryNavigation({
      nav: { canGoNext: opts.canGoNext, canGoPrev: opts.canGoPrev, goNext, goPrev },
      showToast,
      firstMessage: opts.firstMessage,
      lastMessage: opts.lastMessage,
    }),
  );
  return { result, goNext, goPrev, showToast };
}

describe("useViewerBoundaryNavigation", () => {
  test("canGoNext=true のとき handleGoNext は nav.goNext を呼び toast を呼ばない", () => {
    const { result, goNext, showToast } = setup({ canGoNext: true, canGoPrev: true });
    result.current.handleGoNext();
    expect(goNext).toHaveBeenCalledOnce();
    expect(showToast).not.toHaveBeenCalled();
  });

  test("canGoNext=false のとき handleGoNext は toast 表示 + goNext を呼ばない", () => {
    const { result, goNext, showToast } = setup({ canGoNext: false, canGoPrev: true });
    result.current.handleGoNext();
    expect(goNext).not.toHaveBeenCalled();
    expect(showToast).toHaveBeenCalledWith("最後の画像です");
  });

  test("canGoPrev=true のとき handleGoPrev は nav.goPrev を呼び toast を呼ばない", () => {
    const { result, goPrev, showToast } = setup({ canGoNext: true, canGoPrev: true });
    result.current.handleGoPrev();
    expect(goPrev).toHaveBeenCalledOnce();
    expect(showToast).not.toHaveBeenCalled();
  });

  test("canGoPrev=false のとき handleGoPrev は toast 表示 + goPrev を呼ばない", () => {
    const { result, goPrev, showToast } = setup({ canGoNext: true, canGoPrev: false });
    result.current.handleGoPrev();
    expect(goPrev).not.toHaveBeenCalled();
    expect(showToast).toHaveBeenCalledWith("最初の画像です");
  });

  test("firstMessage / lastMessage を上書きできる", () => {
    const { result, showToast } = setup({
      canGoNext: false,
      canGoPrev: false,
      firstMessage: "FIRST",
      lastMessage: "LAST",
    });
    result.current.handleGoNext();
    result.current.handleGoPrev();
    expect(showToast).toHaveBeenNthCalledWith(1, "LAST");
    expect(showToast).toHaveBeenNthCalledWith(2, "FIRST");
  });
});
