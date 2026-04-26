// ツールバーの自動表示/非表示
// - デスクトップ: コンテナ上部 60px 以内にマウスが来たら表示
// - タッチデバイス: 常に表示（pointer: coarse）
// - pointerleave で非表示に戻す
// - コールバック ref でコンテナ要素の遅延マウントに対応（PDF ローディング等）

import { useCallback, useEffect, useState } from "react";

// 上端からの近接閾値（px）
const PROXIMITY_THRESHOLD = 60;

// タッチデバイス判定（呼び出し時に評価）
function detectTouch(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(pointer: coarse)").matches
  );
}

interface UseToolbarAutoHideReturn {
  isToolbarVisible: boolean;
  isTouch: boolean;
  containerCallbackRef: (node: HTMLElement | null) => void;
}

export function useToolbarAutoHide(): UseToolbarAutoHideReturn {
  const [isNearTop, setIsNearTop] = useState(false);
  const [container, setContainer] = useState<HTMLElement | null>(null);
  const isTouch = detectTouch();

  // コールバック ref: DOM 要素のマウント/アンマウントを追跡
  const containerCallbackRef = useCallback((node: HTMLElement | null) => {
    setContainer(node);
  }, []);

  useEffect(() => {
    if (isTouch || !container) {
      return;
    }

    const handlePointerMove = (e: PointerEvent) => {
      const rect = container.getBoundingClientRect();
      const distanceFromTop = e.clientY - rect.top;
      setIsNearTop(distanceFromTop <= PROXIMITY_THRESHOLD);
    };

    const handlePointerLeave = () => {
      setIsNearTop(false);
    };

    container.addEventListener("pointermove", handlePointerMove);
    container.addEventListener("pointerleave", handlePointerLeave);

    return () => {
      container.removeEventListener("pointermove", handlePointerMove);
      container.removeEventListener("pointerleave", handlePointerLeave);
    };
  }, [container, isTouch]);

  return {
    isToolbarVisible: isTouch || isNearTop,
    isTouch,
    containerCallbackRef,
  };
}
