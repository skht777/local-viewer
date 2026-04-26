// CG 系ビューワーの境界チェック付きナビゲーション
// - canGoPrev / canGoNext を尊重し、境界に到達したら toast を表示して停止
// - CgViewer / PdfCgViewer 共通

import { useCallback } from "react";

interface ViewerNavigation {
  canGoPrev: boolean;
  canGoNext: boolean;
  goPrev: () => void;
  goNext: () => void;
}

interface UseViewerBoundaryNavigationParams {
  nav: ViewerNavigation;
  showToast: (message: string, duration?: number) => void;
  firstMessage?: string;
  lastMessage?: string;
}

interface UseViewerBoundaryNavigationResult {
  handleGoNext: () => void;
  handleGoPrev: () => void;
}

const DEFAULT_FIRST_MESSAGE = "最初の画像です";
const DEFAULT_LAST_MESSAGE = "最後の画像です";

export function useViewerBoundaryNavigation({
  nav,
  showToast,
  firstMessage = DEFAULT_FIRST_MESSAGE,
  lastMessage = DEFAULT_LAST_MESSAGE,
}: UseViewerBoundaryNavigationParams): UseViewerBoundaryNavigationResult {
  const handleGoNext = useCallback(() => {
    if (!nav.canGoNext) {
      showToast(lastMessage);
      return;
    }
    nav.goNext();
  }, [nav, showToast, lastMessage]);

  const handleGoPrev = useCallback(() => {
    if (!nav.canGoPrev) {
      showToast(firstMessage);
      return;
    }
    nav.goPrev();
  }, [nav, showToast, firstMessage]);

  return { handleGoNext, handleGoPrev };
}
