// マンガモードのキーボードショートカット
// - react-hotkeys-hook で実装
// - CG モードとは操作体系が異なる（ページ送りではなくスクロール）
// - form 要素フォーカス中は無効化 (enableOnFormTags: false がデフォルト)

import { useHotkeys } from "react-hotkeys-hook";

interface MangaKeyboardCallbacks {
  scrollUp: () => void;
  scrollDown: () => void;
  scrollToTop: () => void;
  scrollToBottom: () => void;
  onEscape: () => void;
  toggleFullscreen: () => void;
  toggleSidebar?: () => void;
  toggleMode?: () => void;
  goNextSet?: () => void;
  goPrevSet?: () => void;
  goNextSetParent?: () => void;
  goPrevSetParent?: () => void;
  zoomIn?: () => void;
  zoomOut?: () => void;
  zoomReset?: () => void;
}

export function useMangaKeyboard(callbacks: MangaKeyboardCallbacks): void {
  // スクロール
  useHotkeys("up, w", () => callbacks.scrollUp(), { preventDefault: true });
  useHotkeys("down, s", () => callbacks.scrollDown(), { preventDefault: true });

  // 先頭/末尾
  useHotkeys("home", () => callbacks.scrollToTop(), { preventDefault: true });
  useHotkeys("end", () => callbacks.scrollToBottom(), { preventDefault: true });

  // サイドバートグル
  useHotkeys("tab", () => callbacks.toggleSidebar?.(), { preventDefault: true });

  // フルスクリーン
  useHotkeys("f", () => callbacks.toggleFullscreen());

  // CG↔マンガモード切替
  useHotkeys("m", () => callbacks.toggleMode?.());

  // Escape（優先順位は呼び出し元で階層化済み）
  useHotkeys("escape", () => callbacks.onEscape());

  // セット間ジャンプ
  useHotkeys("pagedown, x", () => callbacks.goNextSet?.(), { preventDefault: true });
  useHotkeys("pageup, z", () => callbacks.goPrevSet?.(), { preventDefault: true });
  useHotkeys("shift+x", () => callbacks.goNextSetParent?.(), { preventDefault: true });
  useHotkeys("shift+z", () => callbacks.goPrevSetParent?.(), { preventDefault: true });

  // ズーム
  useHotkeys("equal, shift+equal", () => callbacks.zoomIn?.());
  useHotkeys("minus", () => callbacks.zoomOut?.());
  useHotkeys("0", () => callbacks.zoomReset?.());
}
