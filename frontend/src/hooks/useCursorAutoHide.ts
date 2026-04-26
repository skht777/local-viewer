// カーソルオートハイド共通フック
// - targetRef.current のカーソルを `idle 1 秒で none` にする
// - resetCursorTimer() を mousemove / スライダー操作 / クリック等で呼ぶと即時復活 + 再タイマー
// - timeoutMs はデフォルト 1000ms（CG/Manga 両系統で統一）
//
// 抽出元: CgViewer / PdfCgViewer / MangaViewer / PdfMangaViewer の重複コード
// （targetRef は CgViewer/PdfCgViewer は imageAreaRef、MangaViewer/PdfMangaViewer は scrollRef）

import { useCallback, useEffect, useRef } from "react";

interface UseCursorAutoHideOptions {
  timeoutMs?: number;
}

interface UseCursorAutoHideResult {
  resetCursorTimer: () => void;
}

const DEFAULT_TIMEOUT_MS = 1000;

export function useCursorAutoHide(
  targetRef: React.RefObject<HTMLElement | null>,
  options: UseCursorAutoHideOptions = {},
): UseCursorAutoHideResult {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const resetCursorTimer = useCallback(() => {
    if (targetRef.current) {
      targetRef.current.style.cursor = "";
    }
    clearTimeout(cursorTimerRef.current);
    cursorTimerRef.current = setTimeout(() => {
      if (targetRef.current) {
        targetRef.current.style.cursor = "none";
      }
    }, timeoutMs);
  }, [targetRef, timeoutMs]);

  // unmount 時にタイマーリーク防止
  useEffect(() => () => clearTimeout(cursorTimerRef.current), []);

  return { resetCursorTimer };
}
