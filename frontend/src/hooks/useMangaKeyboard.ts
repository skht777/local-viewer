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
  onClose: () => void;
  toggleFullscreen: () => void;
  goNextSet?: () => void;
  goPrevSet?: () => void;
  goNextSetParent?: () => void;
  goPrevSetParent?: () => void;
  toggleHelp?: () => void;
  zoomIn?: () => void;
  zoomOut?: () => void;
  zoomReset?: () => void;
  showTitle?: () => void;
}

export function useMangaKeyboard(callbacks: MangaKeyboardCallbacks): void {
  // スクロール
  useHotkeys("up, w", () => callbacks.scrollUp(), { preventDefault: true });
  useHotkeys("down, s", () => callbacks.scrollDown(), { preventDefault: true });

  // 先頭/末尾
  useHotkeys("home", () => callbacks.scrollToTop(), { preventDefault: true });
  useHotkeys("end", () => callbacks.scrollToBottom(), { preventDefault: true });

  // フルスクリーン
  useHotkeys("f", () => callbacks.toggleFullscreen());

  // Escape（ダイアログ閉じのみ）
  useHotkeys("escape", () => callbacks.onEscape());

  // ビューワーを閉じる
  useHotkeys("b", () => callbacks.onClose());

  // セット間ジャンプ
  useHotkeys("pagedown, x", () => callbacks.goNextSet?.(), { preventDefault: true });
  useHotkeys("pageup, z", () => callbacks.goPrevSet?.(), { preventDefault: true });
  useHotkeys("shift+x", () => callbacks.goNextSetParent?.(), { preventDefault: true });
  useHotkeys("shift+z", () => callbacks.goPrevSetParent?.(), { preventDefault: true });

  // ヘルプ表示切替
  useHotkeys("shift+/", () => callbacks.toggleHelp?.());

  // ズーム
  useHotkeys("equal, shift+equal", () => callbacks.zoomIn?.());
  useHotkeys("minus", () => callbacks.zoomOut?.());
  useHotkeys("0", () => callbacks.zoomReset?.());

  // タイトルポップアップ表示
  useHotkeys("m", () => callbacks.showTitle?.());
}
