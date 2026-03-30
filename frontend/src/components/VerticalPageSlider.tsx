// 画面右端にフェードインする縦ページスライダー（マンガモード用）
// - ビューワーコンテナの pointermove で右端との距離を閾値判定
// - マウス: 右端に近づくとフェードイン、離れるとフェードアウト
// - ドラッグ中: 表示維持
// - キーボード: focus-within で常時表示
// - タッチ: matchMedia("(pointer: coarse)") で常時表示
// - 非表示時は pointer-events-none でクリックを透過
// - 縦方向: writing-mode + orient 属性で Firefox 対応

import { useCallback, useEffect, useRef, useState } from "react";

interface VerticalPageSliderProps {
  currentIndex: number;
  totalCount: number;
  onGoTo: (index: number) => void;
  // ビューワーコンテナの ref（pointermove 検出用）
  containerRef: React.RefObject<HTMLElement | null>;
  // カーソルオートハイドとの統合
  onSliderActivity?: () => void;
}

// 右端からの近接閾値（px）
const PROXIMITY_THRESHOLD = 60;

// 初回ヒント表示用 sessionStorage キー（PageSlider と共有）
const HINT_KEY = "slider-hint-shown";

// タッチデバイス判定
const isTouchDevice =
  typeof window !== "undefined" &&
  typeof window.matchMedia === "function" &&
  window.matchMedia("(pointer: coarse)").matches;

export function VerticalPageSlider({
  currentIndex,
  totalCount,
  onGoTo,
  containerRef,
  onSliderActivity,
}: VerticalPageSliderProps) {
  const [isNearRight, setIsNearRight] = useState(false);
  const [isFocused, setIsFocused] = useState(false);
  const isDragging = useRef(false);

  // 初回表示ヒント: セッション内で初回のみ2秒間スライダーを表示
  const [isHintVisible, setIsHintVisible] = useState(() => {
    return !sessionStorage.getItem(HINT_KEY);
  });

  useEffect(() => {
    if (!isHintVisible) return;
    sessionStorage.setItem(HINT_KEY, "1");
    const timer = setTimeout(() => setIsHintVisible(false), 2000);
    return () => clearTimeout(timer);
  }, [isHintVisible]);

  // コンテナの pointermove で右端近接を検出
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handlePointerMove = (e: PointerEvent) => {
      const rect = container.getBoundingClientRect();
      const distanceFromRight = rect.right - e.clientX;

      if (distanceFromRight <= PROXIMITY_THRESHOLD) {
        setIsNearRight(true);
        onSliderActivity?.();
      } else if (!isDragging.current) {
        setIsNearRight(false);
      }
    };

    const handlePointerLeave = () => {
      if (!isDragging.current) {
        setIsNearRight(false);
      }
    };

    container.addEventListener("pointermove", handlePointerMove);
    container.addEventListener("pointerleave", handlePointerLeave);

    return () => {
      container.removeEventListener("pointermove", handlePointerMove);
      container.removeEventListener("pointerleave", handlePointerLeave);
    };
  }, [containerRef, onSliderActivity]);

  const handlePointerDown = useCallback(() => {
    isDragging.current = true;
    onSliderActivity?.();
  }, [onSliderActivity]);

  const handlePointerUp = useCallback(() => {
    isDragging.current = false;
  }, []);

  if (totalCount <= 1) return null;

  const isVisible = isTouchDevice || isNearRight || isFocused || isHintVisible;

  return (
    <div
      data-testid="page-slider"
      className={`absolute top-4 right-4 bottom-4 z-20 flex items-center transition-opacity duration-300 ${
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
        // orient 属性は Firefox 固有（他ブラウザでは無視される）
        // eslint-disable-next-line react/no-unknown-property
        {...({ orient: "vertical" } as React.InputHTMLAttributes<HTMLInputElement>)}
        aria-label="ページスライダー"
        className="h-full"
        style={{ writingMode: "vertical-lr", direction: "rtl" }}
      />
    </div>
  );
}
