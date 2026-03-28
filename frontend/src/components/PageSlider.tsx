// 画面下部にフェードインするページスライダー（CG モード用）
// - ビューワーコンテナの pointermove で下端との距離を閾値判定
// - マウス: 下端に近づくとフェードイン、離れるとフェードアウト
// - ドラッグ中: 表示維持
// - キーボード: focus-within で常時表示
// - タッチ: matchMedia("(pointer: coarse)") で常時表示
// - 非表示時は pointer-events-none でクリックを透過

import { useCallback, useEffect, useRef, useState } from "react";

interface PageSliderProps {
  currentIndex: number;
  totalCount: number;
  onGoTo: (index: number) => void;
  // ビューワーコンテナの ref（pointermove 検出用）
  containerRef: React.RefObject<HTMLElement | null>;
  // カーソルオートハイドとの統合: スライダー操作中に呼ぶ
  onSliderActivity?: () => void;
}

// 下端からの近接閾値（px）
const PROXIMITY_THRESHOLD = 80;

// タッチデバイス判定
const isTouchDevice =
  typeof window !== "undefined" &&
  typeof window.matchMedia === "function" &&
  window.matchMedia("(pointer: coarse)").matches;

export function PageSlider({
  currentIndex,
  totalCount,
  onGoTo,
  containerRef,
  onSliderActivity,
}: PageSliderProps) {
  const [isNearBottom, setIsNearBottom] = useState(false);
  const [isFocused, setIsFocused] = useState(false);
  const isDragging = useRef(false);

  // コンテナに pointermove/pointerleave リスナーを設定
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handlePointerMove = (e: PointerEvent) => {
      const rect = container.getBoundingClientRect();
      const distanceFromBottom = rect.bottom - e.clientY;

      if (distanceFromBottom <= PROXIMITY_THRESHOLD) {
        setIsNearBottom(true);
        onSliderActivity?.();
      } else if (!isDragging.current) {
        setIsNearBottom(false);
      }
    };

    const handlePointerLeave = () => {
      if (!isDragging.current) {
        setIsNearBottom(false);
      }
    };

    container.addEventListener("pointermove", handlePointerMove);
    container.addEventListener("pointerleave", handlePointerLeave);

    return () => {
      container.removeEventListener("pointermove", handlePointerMove);
      container.removeEventListener("pointerleave", handlePointerLeave);
    };
  }, [containerRef, onSliderActivity]);

  // ドラッグ開始
  const handlePointerDown = useCallback(() => {
    isDragging.current = true;
    onSliderActivity?.();
  }, [onSliderActivity]);

  // ドラッグ終了
  const handlePointerUp = useCallback(() => {
    isDragging.current = false;
  }, []);

  if (totalCount <= 1) return null;

  const isVisible = isTouchDevice || isNearBottom || isFocused;

  return (
    <div
      data-testid="page-slider"
      className={`absolute right-4 bottom-4 left-4 z-20 transition-opacity duration-300 ${
        isVisible ? "opacity-100" : "pointer-events-none opacity-0"
      }`}
    >
      <input
        type="range"
        min={0}
        max={totalCount - 1}
        value={currentIndex}
        onChange={(e) => onGoTo(Number(e.target.value))}
        onPointerDown={handlePointerDown}
        onPointerUp={handlePointerUp}
        onFocus={() => setIsFocused(true)}
        onBlur={() => setIsFocused(false)}
        aria-label="ページスライダー"
        className="w-full"
      />
    </div>
  );
}
