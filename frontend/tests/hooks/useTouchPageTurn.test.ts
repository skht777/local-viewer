// useTouchPageTurn の振る舞い検証
// - 水平スワイプで onSwipeLeft/onSwipeRight が呼ばれる
// - 縦移動優位時は不発火
// - しきい値未満は不発火
// - スワイプ後の click が抑制される
// - enabled=false で全 handler が no-op
// - touch 以外の pointerType (mouse) は通常クリック経路に任せる (=swipe 不発火)

import { renderHook, act } from "@testing-library/react";
import { useTouchPageTurn } from "../../src/hooks/useTouchPageTurn";

interface MakeOpts {
  x: number;
  y: number;
  pointerId?: number;
  pointerType?: string;
}

function makePointer({ x, y, pointerId = 1, pointerType = "touch" }: MakeOpts) {
  return {
    clientX: x,
    clientY: y,
    pointerId,
    pointerType,
    currentTarget: {
      setPointerCapture: () => {},
    },
  } as unknown as React.PointerEvent<HTMLDivElement>;
}

function makeClickEvent() {
  return {
    preventDefault: () => {},
    stopPropagation: () => {},
  } as unknown as React.MouseEvent<HTMLDivElement>;
}

describe("useTouchPageTurn", () => {
  test("水平左スワイプ (dx < 0, |dx|>=50, |dx|>2|dy|) で onSwipeLeft が呼ばれる", () => {
    const onSwipeLeft = vi.fn();
    const onSwipeRight = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft, onSwipeRight }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 300, y: 200 }));
      result.current.onPointerUp(makePointer({ x: 200, y: 210 }));
    });
    expect(onSwipeLeft).toHaveBeenCalledOnce();
    expect(onSwipeRight).not.toHaveBeenCalled();
  });

  test("水平右スワイプ (dx > 0) で onSwipeRight が呼ばれる", () => {
    const onSwipeLeft = vi.fn();
    const onSwipeRight = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft, onSwipeRight }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 100, y: 200 }));
      result.current.onPointerUp(makePointer({ x: 200, y: 195 }));
    });
    expect(onSwipeRight).toHaveBeenCalledOnce();
    expect(onSwipeLeft).not.toHaveBeenCalled();
  });

  test("縦移動優位時 (|dy| > |dx|/2 相当) は swipe 不発火", () => {
    const onSwipeLeft = vi.fn();
    const onSwipeRight = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft, onSwipeRight }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 100, y: 100 }));
      // dx=60, dy=50 → |dx|=60 だが 2*|dy|=100 > 60 なので不発火
      result.current.onPointerUp(makePointer({ x: 160, y: 150 }));
    });
    expect(onSwipeLeft).not.toHaveBeenCalled();
    expect(onSwipeRight).not.toHaveBeenCalled();
  });

  test("しきい値未満 (|dx|<50) は swipe 不発火", () => {
    const onSwipeLeft = vi.fn();
    const onSwipeRight = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft, onSwipeRight }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 100, y: 100 }));
      result.current.onPointerUp(makePointer({ x: 140, y: 100 }));
    });
    expect(onSwipeLeft).not.toHaveBeenCalled();
    expect(onSwipeRight).not.toHaveBeenCalled();
  });

  test("swipe 成立後の click は onClickCapture で preventDefault + stopPropagation される", () => {
    const onSwipeLeft = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft, onSwipeRight: vi.fn() }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 300, y: 200 }));
      result.current.onPointerUp(makePointer({ x: 200, y: 210 }));
    });
    expect(onSwipeLeft).toHaveBeenCalledOnce();

    const clickEvent = makeClickEvent();
    const preventSpy = vi.spyOn(clickEvent, "preventDefault");
    const stopSpy = vi.spyOn(clickEvent, "stopPropagation");
    act(() => {
      result.current.onClickCapture(clickEvent);
    });
    expect(preventSpy).toHaveBeenCalled();
    expect(stopSpy).toHaveBeenCalled();
  });

  test("swipe していない場合の click は通常通り通過 (抑制なし)", () => {
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft: vi.fn(), onSwipeRight: vi.fn() }),
    );
    const clickEvent = makeClickEvent();
    const preventSpy = vi.spyOn(clickEvent, "preventDefault");
    const stopSpy = vi.spyOn(clickEvent, "stopPropagation");
    act(() => {
      result.current.onClickCapture(clickEvent);
    });
    expect(preventSpy).not.toHaveBeenCalled();
    expect(stopSpy).not.toHaveBeenCalled();
  });

  test("enabled=false で全 handler が no-op (swipe 判定なし)", () => {
    const onSwipeLeft = vi.fn();
    const onSwipeRight = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: false, onSwipeLeft, onSwipeRight }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 300, y: 200 }));
      result.current.onPointerUp(makePointer({ x: 200, y: 210 }));
    });
    expect(onSwipeLeft).not.toHaveBeenCalled();
    expect(onSwipeRight).not.toHaveBeenCalled();
  });

  test("pointerType=mouse の pointer は無視される (通常クリックに任せる)", () => {
    const onSwipeLeft = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft, onSwipeRight: vi.fn() }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 300, y: 200, pointerType: "mouse" }));
      result.current.onPointerUp(makePointer({ x: 200, y: 210, pointerType: "mouse" }));
    });
    expect(onSwipeLeft).not.toHaveBeenCalled();
  });

  test("pointercancel で開始情報がクリアされ、後続 pointerup で swipe 判定されない", () => {
    const onSwipeLeft = vi.fn();
    const { result } = renderHook(() =>
      useTouchPageTurn({ enabled: true, onSwipeLeft, onSwipeRight: vi.fn() }),
    );
    act(() => {
      result.current.onPointerDown(makePointer({ x: 300, y: 200 }));
      result.current.onPointerCancel(makePointer({ x: 300, y: 200 }));
      result.current.onPointerUp(makePointer({ x: 200, y: 210 }));
    });
    expect(onSwipeLeft).not.toHaveBeenCalled();
  });
});
