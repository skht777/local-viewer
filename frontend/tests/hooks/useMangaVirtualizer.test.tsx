// useMangaVirtualizer の振る舞い検証
// - useVirtualizer の options をキャプチャして検証
// - estimateSize: pageSizes 指定時は ps.height/ps.width 比率、それ以外は 3:4
// - initialIndex > 0 で 1 度だけ scrollToIndex
// - pageSizesReady=true で virtualizer.measure() 再実行
// - zoomLevel 変動で anchorIndexRef を起点に scrollToIndex

import { renderHook } from "@testing-library/react";
import { useRef } from "react";
import { useMangaVirtualizer } from "../../src/hooks/useMangaVirtualizer";

// useVirtualizer をモック化して options をキャプチャ
interface CapturedOptions {
  count: number;
  estimateSize: (index: number) => number;
  getScrollElement: () => HTMLDivElement | null;
  overscan?: number;
}
let lastOptions: CapturedOptions | null = null;
const measure = vi.fn();
const scrollToIndex = vi.fn();

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: (options: CapturedOptions) => {
    lastOptions = options;
    return {
      measure,
      scrollToIndex,
      getVirtualItems: () => [],
      getTotalSize: () => 0,
    };
  },
}));

beforeEach(() => {
  lastOptions = null;
  measure.mockClear();
  scrollToIndex.mockClear();
});

/* oxlint-disable promise/prefer-await-to-callbacks -- requestAnimationFrame の API は callback ベース */
function setupRaf() {
  // requestAnimationFrame を即時実行に置換
  return vi
    .spyOn(globalThis, "requestAnimationFrame")
    .mockImplementation((cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    });
}
/* oxlint-enable promise/prefer-await-to-callbacks */

describe("useMangaVirtualizer - estimateSize", () => {
  test("pageSizes 未指定なら 3:4 比率（containerWidth × 4/3）", () => {
    renderHook(() => useMangaVirtualizer({ count: 5, zoomLevel: 100, initialIndex: 0 }));
    // scrollRef はまだ DOM に attach されていないので clientWidth=undefined → DEFAULT(800)
    // 800 × 100/100 × 4/3 ≈ 1066.67
    const size = lastOptions!.estimateSize(0);
    expect(Math.round(size)).toBe(1067);
  });

  test("pageSizes と pageSizesReady=true があれば実比率を使う", () => {
    renderHook(() =>
      useMangaVirtualizer({
        count: 1,
        zoomLevel: 100,
        initialIndex: 0,
        pageSizes: [{ width: 100, height: 200 }],
        pageSizesReady: true,
      }),
    );
    // 800 × 100/100 × (200/100) = 1600
    expect(lastOptions!.estimateSize(0)).toBe(1600);
  });

  test("pageSizesReady=false なら pageSizes があっても fallback", () => {
    renderHook(() =>
      useMangaVirtualizer({
        count: 1,
        zoomLevel: 100,
        initialIndex: 0,
        pageSizes: [{ width: 100, height: 200 }],
        pageSizesReady: false,
      }),
    );
    expect(Math.round(lastOptions!.estimateSize(0))).toBe(1067);
  });

  test("zoomLevel が反映される（zoomLevel=200 で 2 倍）", () => {
    renderHook(() => useMangaVirtualizer({ count: 1, zoomLevel: 200, initialIndex: 0 }));
    // 800 × 200/100 × 4/3 ≈ 2133.33
    expect(Math.round(lastOptions!.estimateSize(0))).toBe(2133);
  });
});

describe("useMangaVirtualizer - scrollToIndex / measure", () => {
  test("initialIndex > 0 のとき初期 scrollToIndex が 1 回呼ばれる", () => {
    renderHook(() => useMangaVirtualizer({ count: 10, zoomLevel: 100, initialIndex: 5 }));
    expect(scrollToIndex).toHaveBeenCalledWith(5, { align: "start" });
  });

  test("initialIndex=0 のときは scrollToIndex を呼ばない", () => {
    renderHook(() => useMangaVirtualizer({ count: 10, zoomLevel: 100, initialIndex: 0 }));
    expect(scrollToIndex).not.toHaveBeenCalled();
  });

  test("count=0 のときは scrollToIndex を呼ばない", () => {
    renderHook(() => useMangaVirtualizer({ count: 0, zoomLevel: 100, initialIndex: 5 }));
    expect(scrollToIndex).not.toHaveBeenCalled();
  });

  test("初期スクロールは 1 回限り（再 render しても 2 回目は呼ばれない）", () => {
    const { rerender } = renderHook(
      ({ initialIndex }: { initialIndex: number }) =>
        useMangaVirtualizer({ count: 10, zoomLevel: 100, initialIndex }),
      { initialProps: { initialIndex: 5 } },
    );
    expect(scrollToIndex).toHaveBeenCalledOnce();
    rerender({ initialIndex: 7 });
    // initialScrollDone ref により 2 回目は呼ばれない
    expect(scrollToIndex).toHaveBeenCalledOnce();
  });

  test("pageSizesReady=true への遷移で measure が呼ばれる", () => {
    const { rerender } = renderHook(
      ({ ready }: { ready: boolean }) =>
        useMangaVirtualizer({
          count: 1,
          zoomLevel: 100,
          initialIndex: 0,
          pageSizes: [{ width: 100, height: 200 }],
          pageSizesReady: ready,
        }),
      { initialProps: { ready: false } },
    );
    measure.mockClear();
    rerender({ ready: true });
    expect(measure).toHaveBeenCalled();
  });
});

describe("useMangaVirtualizer - zoom anchor", () => {
  test("zoomLevel 変動で measure + scrollToIndex(anchorIndexRef.current) が呼ばれる", () => {
    const raf = setupRaf();
    function useProbe({ zoom }: { zoom: number }) {
      const anchorIndexRef = useRef(7);
      return useMangaVirtualizer({
        count: 10,
        zoomLevel: zoom,
        initialIndex: 0,
        anchorIndexRef,
      });
    }
    const { rerender } = renderHook(({ zoom }: { zoom: number }) => useProbe({ zoom }), {
      initialProps: { zoom: 100 },
    });
    measure.mockClear();
    scrollToIndex.mockClear();

    rerender({ zoom: 150 });
    expect(measure).toHaveBeenCalled();
    expect(scrollToIndex).toHaveBeenCalledWith(7, { align: "start" });
    raf.mockRestore();
  });

  test("anchorIndexRef を渡さないと zoomLevel 変動でも scrollToIndex は呼ばれない", () => {
    const raf = setupRaf();
    const { rerender } = renderHook(
      ({ zoom }: { zoom: number }) =>
        useMangaVirtualizer({ count: 10, zoomLevel: zoom, initialIndex: 0 }),
      { initialProps: { zoom: 100 } },
    );
    scrollToIndex.mockClear();
    rerender({ zoom: 200 });
    expect(scrollToIndex).not.toHaveBeenCalled();
    raf.mockRestore();
  });

  test("zoomLevel が変わらないなら何もしない", () => {
    const raf = setupRaf();
    function useProbe({ zoom }: { zoom: number }) {
      const anchorIndexRef = useRef(3);
      return useMangaVirtualizer({
        count: 5,
        zoomLevel: zoom,
        initialIndex: 0,
        anchorIndexRef,
      });
    }
    const { rerender } = renderHook(({ zoom }: { zoom: number }) => useProbe({ zoom }), {
      initialProps: { zoom: 100 },
    });
    measure.mockClear();
    scrollToIndex.mockClear();
    rerender({ zoom: 100 });
    expect(measure).not.toHaveBeenCalled();
    expect(scrollToIndex).not.toHaveBeenCalled();
    raf.mockRestore();
  });
});
