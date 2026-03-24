// Fullscreen API ラッパー
// - document.fullscreenElement で状態を判定
// - fullscreenchange イベントで React 状態と同期

import { useCallback, useEffect, useState } from "react";

interface UseFullscreenReturn {
  isFullscreen: boolean;
  toggleFullscreen: () => void;
}

export function useFullscreen(): UseFullscreenReturn {
  const [isFullscreen, setIsFullscreen] = useState(() => document.fullscreenElement !== null);

  useEffect(() => {
    const handler = () => {
      setIsFullscreen(document.fullscreenElement !== null);
    };
    document.addEventListener("fullscreenchange", handler);
    return () => document.removeEventListener("fullscreenchange", handler);
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) {
      document.exitFullscreen();
    } else {
      document.documentElement.requestFullscreen();
    }
  }, []);

  return { isFullscreen, toggleFullscreen };
}
