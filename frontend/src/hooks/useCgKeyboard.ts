// CGモードのキーボードショートカット
// - react-hotkeys-hook で実装
// - form 要素フォーカス中は無効化 (enableOnFormTags: false がデフォルト)

import { useHotkeys } from "react-hotkeys-hook";

interface CgKeyboardCallbacks {
  goNext: () => void;
  goPrev: () => void;
  goFirst: () => void;
  goLast: () => void;
  onClose: () => void;
  toggleFullscreen: () => void;
  setFitWidth: () => void;
  setFitHeight: () => void;
  cycleSpread: () => void;
  scrollUp: () => void;
  scrollDown: () => void;
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

  // ビューワーを閉じる
  useHotkeys("escape", () => callbacks.onClose());
}
