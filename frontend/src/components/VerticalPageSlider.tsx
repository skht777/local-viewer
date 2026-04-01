// 画面右端にフェードインする縦ページスライダー（マンガモード用）
// - ビューワーコンテナの pointermove で右端との距離を閾値判定
// - マウス: 右端に近づくとフェードイン、離れるとフェードアウト
// - ホバー: スライダー上にポインタがある間は表示維持（pointerenter/pointerleave）
// - ドラッグ中: 表示維持（document レベルで pointerup を監視）
// - キーボード: focus-within で常時表示
// - タッチ: matchMedia("(pointer: coarse)") で常時表示
// - 非表示時は pointer-events-none でクリックを透過
// - 縦方向: writing-mode: vertical-lr で標準準拠の縦スライダー

import { useCallback, useEffect, useRef, useState } from "react";
import { SliderTooltip } from "./SliderTooltip";

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
  const [isHovering, setIsHovering] = useState(false);
  const isDragging = useRef(false);
  const pointerLeftDuringDrag = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);

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

  // ドラッグ開始: document レベルで pointerup を監視
  const handlePointerDown = useCallback(() => {
    isDragging.current = true;
    pointerLeftDuringDrag.current = false;
    onSliderActivity?.();

    const onUp = () => {
      isDragging.current = false;
      if (pointerLeftDuringDrag.current) {
        pointerLeftDuringDrag.current = false;
        setIsHovering(false);
      }
      document.removeEventListener("pointerup", onUp);
    };
    document.addEventListener("pointerup", onUp);
  }, [onSliderActivity]);

  if (totalCount <= 1) return null;

  const isVisible = isTouchDevice || isNearRight || isFocused || isHintVisible || isHovering;

  // サム位置の計算（inputの高さに対する割合で算出）
  const thumbPosition = (() => {
    if (!inputRef.current || totalCount <= 1) return 0;
    const ratio = currentIndex / (totalCount - 1);
    return ratio * inputRef.current.offsetHeight;
  })();

  return (
    <div
      data-testid="page-slider"
      className={`absolute top-4 right-4 bottom-4 z-20 flex items-center transition-opacity duration-300 ${
        isVisible ? "opacity-100" : "pointer-events-none opacity-0"
      }`}
      onPointerEnter={() => setIsHovering(true)}
      onPointerLeave={() => {
        if (isDragging.current) {
          pointerLeftDuringDrag.current = true;
        } else {
          setIsHovering(false);
        }
      }}
    >
      <div className="relative h-full">
        <SliderTooltip
          currentIndex={currentIndex}
          totalCount={totalCount}
          position={thumbPosition}
          orientation="vertical"
          visible={isHovering}
        />
        <input
          ref={inputRef}
          type="range"
          min={0}
          max={totalCount - 1}
          value={currentIndex}
          onChange={(e) => onGoTo(Number(e.target.value))}
          onPointerDown={handlePointerDown}
          onFocus={() => setIsFocused(true)}
          onBlur={() => setIsFocused(false)}
          aria-label="ページスライダー"
          className="h-full"
          style={{ writingMode: "vertical-lr" }}
        />
      </div>
    </div>
  );
}
