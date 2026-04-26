// CGモードのキーボードショートカット
// - react-hotkeys-hook で実装
// - form 要素フォーカス中は無効化 (enableOnFormTags: false がデフォルト)

import { useHotkeys } from "react-hotkeys-hook";

interface CgKeyboardCallbacks {
  goNext: () => void;
  goPrev: () => void;
  goFirst: () => void;
  goLast: () => void;
  onEscape: () => void;
  onClose: () => void;
  toggleFullscreen: () => void;
  setFitWidth: () => void;
  setFitHeight: () => void;
  cycleSpread: () => void;
  scrollUp: () => void;
  scrollDown: () => void;
  goNextSet?: () => void;
  goPrevSet?: () => void;
  goNextSetParent?: () => void;
  goPrevSetParent?: () => void;
  toggleHelp?: () => void;
  showTitle?: () => void;
}

export function useCgKeyboard(callbacks: CgKeyboardCallbacks): void {
  // ページ送り
  useHotkeys("right, d", () => callbacks.goNext(), { preventDefault: true });
  useHotkeys("left, a", () => callbacks.goPrev(), { preventDefault: true });

  // スクロール（画像がビューポートをはみ出す場合用）
  useHotkeys("up, w", () => callbacks.scrollUp(), { preventDefault: true });
  useHotkeys("down, s", () => callbacks.scrollDown(), { preventDefault: true });

  // 先頭/末尾
  useHotkeys("home", () => callbacks.goFirst(), { preventDefault: true });
  useHotkeys("end", () => callbacks.goLast(), { preventDefault: true });

  // フィット切替
  useHotkeys("v", () => callbacks.setFitWidth());
  useHotkeys("h", () => callbacks.setFitHeight());

  // 見開き切替
  useHotkeys("q", () => callbacks.cycleSpread());

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

  // タイトルポップアップ表示
  useHotkeys("m", () => callbacks.showTitle?.());
}
