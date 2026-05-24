// タッチ水平スワイプでページ送り
// - pointerdown → pointermove → pointerup で水平スワイプ判定
//   (横移動 ≥ swipeThreshold かつ |横| > 2 * |縦| の場合のみ swipe 成立)
// - swipe 成立後の click は onClickCapture で 1 回だけ抑制 (二重発火防止)
// - pointercancel / lostpointercapture では pointer 開始情報のみリセットし、
//   swipedRef は触らない (合成 click が先に来る順序を保証するため)
// - swipedRef はフェイルセーフとして setTimeout(0) で遅延クリア
//   (queueMicrotask は使用禁止: 合成 click より microtask が先に走り抑制が抜ける)
// - enabled=false で全 handler が no-op (デスクトップ時に組み込んでも副作用なし)
//
// 戻り値は `<div {...handlers}>` で spread して利用する想定。

import { useCallback, useRef } from "react";

interface PointerStart {
  x: number;
  y: number;
  pointerId: number;
}

interface UseTouchPageTurnArgs {
  enabled: boolean;
  onSwipeLeft: () => void; // 左スワイプ (dx < 0): 次ページ (画像が左に流れる視覚)
  onSwipeRight: () => void; // 右スワイプ (dx > 0): 前ページ
  swipeThreshold?: number;
}

interface TouchPageTurnHandlers {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: React.PointerEvent<HTMLDivElement>) => void;
  onPointerCancel: (e: React.PointerEvent<HTMLDivElement>) => void;
  onLostPointerCapture: (e: React.PointerEvent<HTMLDivElement>) => void;
  onClickCapture: (e: React.MouseEvent<HTMLDivElement>) => void;
}

export function useTouchPageTurn({
  enabled,
  onSwipeLeft,
  onSwipeRight,
  swipeThreshold = 50,
}: UseTouchPageTurnArgs): TouchPageTurnHandlers {
  const startRef = useRef<PointerStart | null>(null);
  const swipedRef = useRef(false);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!enabled) {
        return;
      }
      // タッチ以外 (mouse/pen) は通常クリック経路に任せる
      if (e.pointerType !== "touch") {
        return;
      }
      startRef.current = { x: e.clientX, y: e.clientY, pointerId: e.pointerId };
    },
    [enabled],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!enabled || !startRef.current || e.pointerId !== startRef.current.pointerId) {
        return;
      }
      const dx = e.clientX - startRef.current.x;
      const dy = e.clientY - startRef.current.y;
      // 水平スワイプ判定: しきい値超過 + 横が縦の 2 倍以上
      // pointermove 段階でキャプチャしておくと pointerup までブラウザのスクロール処理に
      // 食われにくくなる
      if (Math.abs(dx) >= swipeThreshold && Math.abs(dx) > 2 * Math.abs(dy)) {
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {
          // pointer がすでに解放されている等の例外は無視 (UI 影響なし)
        }
      }
    },
    [enabled, swipeThreshold],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      const start = startRef.current;
      if (!enabled || !start || e.pointerId !== start.pointerId) {
        startRef.current = null;
        return;
      }
      const dx = e.clientX - start.x;
      const dy = e.clientY - start.y;
      startRef.current = null;

      if (Math.abs(dx) >= swipeThreshold && Math.abs(dx) > 2 * Math.abs(dy)) {
        swipedRef.current = true;
        if (dx < 0) {
          onSwipeLeft();
        } else {
          onSwipeRight();
        }
        // フェイルセーフ: 合成 click が来なかった場合に備えて遅延クリア
        // setTimeout(0) は task キュー (macrotask) で、pointerup 後の click より遅延する
        setTimeout(() => {
          swipedRef.current = false;
        }, 0);
      }
    },
    [enabled, swipeThreshold, onSwipeLeft, onSwipeRight],
  );

  // pointercancel / lostpointercapture: 開始情報のみリセット
  // (swipedRef を触ると次の click 抑制が抜けるため触らない)
  const resetStart = useCallback(() => {
    startRef.current = null;
  }, []);

  const onClickCapture = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (swipedRef.current) {
      e.preventDefault();
      e.stopPropagation();
      swipedRef.current = false;
    }
  }, []);

  return {
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onPointerCancel: resetStart,
    onLostPointerCapture: resetStart,
    onClickCapture,
  };
}
