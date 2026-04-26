// PdfCgViewer のコンテナサイズ計測（ResizeObserver で動的追従）
// - combinedRef を div に渡すと初期サイズと resize 時の幅/高さを track
// - imageAreaRef を返し、scrollBy / PageSlider 等で参照可能にする
// - cleanup: unmount 時に Observer disconnect

import { useCallback, useEffect, useRef, useState } from "react";

interface ContainerSize {
  width: number;
  height: number;
}

interface UsePdfContainerSizeResult {
  containerSize: ContainerSize;
  imageAreaRef: React.RefObject<HTMLDivElement | null>;
  combinedRef: (node: HTMLDivElement | null) => void;
}

const DEFAULT_WIDTH = 800;
const DEFAULT_HEIGHT = 600;

export function usePdfContainerSize(): UsePdfContainerSizeResult {
  const imageAreaRef = useRef<HTMLDivElement | null>(null);
  const [containerSize, setContainerSize] = useState<ContainerSize>({
    width: DEFAULT_WIDTH,
    height: DEFAULT_HEIGHT,
  });
  const resizeObserverRef = useRef<ResizeObserver | null>(null);

  const combinedRef = useCallback((node: HTMLDivElement | null) => {
    // 既存の Observer をクリーンアップ
    resizeObserverRef.current?.disconnect();

    imageAreaRef.current = node;
    if (!node) {
      return;
    }

    // 初期サイズ
    const w = node.clientWidth || DEFAULT_WIDTH;
    const h = node.clientHeight || DEFAULT_HEIGHT;
    setContainerSize({ width: w, height: h });

    // ResizeObserver で動的追従
    resizeObserverRef.current = new ResizeObserver((entries) => {
      const [entry] = entries;
      if (!entry) {
        return;
      }
      const { width, height } = entry.contentRect;
      setContainerSize((prev) => {
        if (prev.width === width && prev.height === height) {
          return prev;
        }
        return { width, height };
      });
    });
    resizeObserverRef.current.observe(node);
  }, []);

  // unmount 時のクリーンアップ
  useEffect(() => () => resizeObserverRef.current?.disconnect(), []);

  return { containerSize, imageAreaRef, combinedRef };
}
