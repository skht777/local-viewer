// ツールバーの自動表示/非表示
// - デスクトップ: コンテナ上部 60px 以内にマウスが来たら表示
// - タッチデバイス: 常に表示（pointer: coarse）
// - pointerleave で非表示に戻す

import { useEffect, useState } from "react";

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
}

export function useToolbarAutoHide(
  containerRef: React.RefObject<HTMLElement | null>,
): UseToolbarAutoHideReturn {
  const [isNearTop, setIsNearTop] = useState(false);
  const isTouch = detectTouch();

  useEffect(() => {
    if (isTouch) return;

    const container = containerRef.current;
    if (!container) return;

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
  }, [containerRef]);

  return {
    isToolbarVisible: isTouch || isNearTop,
    isTouch,
  };
}
