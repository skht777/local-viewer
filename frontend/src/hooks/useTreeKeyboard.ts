// ディレクトリツリーのキーボード操作（WAI-ARIA TreeView パターン）
// - ↑/↓ で可視ノード間移動
// - → で展開 / ← で折りたたみ
// - Enter でナビゲーション実行
// - Home/End で最初/最後の可視ノードへ
// - t でファイルブラウザーにフォーカス移動
// - enabled で排他制御（ブラウザーフォーカス中は無効化）

import { useHotkeys } from "react-hotkeys-hook";

export interface TreeKeyboardCallbacks {
  moveUp: () => void;
  moveDown: () => void;
  expand: () => void;
  collapse: () => void;
  select: () => void;
  goFirst: () => void;
  goLast: () => void;
  focusBrowser: () => void;
}

export function useTreeKeyboard(callbacks: TreeKeyboardCallbacks, enabled: boolean): void {
  useHotkeys("down", () => callbacks.moveDown(), { preventDefault: true, enabled });
  useHotkeys("up", () => callbacks.moveUp(), { preventDefault: true, enabled });
  useHotkeys("right", () => callbacks.expand(), { preventDefault: true, enabled });
  useHotkeys("left", () => callbacks.collapse(), { preventDefault: true, enabled });
  useHotkeys("enter", () => callbacks.select(), { preventDefault: true, enabled });
  useHotkeys("home", () => callbacks.goFirst(), { preventDefault: true, enabled });
  useHotkeys("end", () => callbacks.goLast(), { preventDefault: true, enabled });
  useHotkeys("t", () => callbacks.focusBrowser(), { enabled });
}
