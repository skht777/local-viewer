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
  toggleFullscreen: () => void;
  setFitWidth: () => void;
  setFitHeight: () => void;
  cycleSpread: () => void;
  scrollUp: () => void;
  scrollDown: () => void;
  toggleMode?: () => void;
  goNextSet?: () => void;
  goPrevSet?: () => void;
  goNextSetParent?: () => void;
  goPrevSetParent?: () => void;
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

  // CG↔マンガモード切替
  useHotkeys("m", () => callbacks.toggleMode?.());

  // Escape（優先順位は呼び出し元で階層化済み）
  useHotkeys("escape", () => callbacks.onEscape());

  // セット間ジャンプ
  useHotkeys("pagedown, x", () => callbacks.goNextSet?.(), { preventDefault: true });
  useHotkeys("pageup, z", () => callbacks.goPrevSet?.(), { preventDefault: true });
  useHotkeys("shift+x", () => callbacks.goNextSetParent?.(), { preventDefault: true });
  useHotkeys("shift+z", () => callbacks.goPrevSetParent?.(), { preventDefault: true });
}
