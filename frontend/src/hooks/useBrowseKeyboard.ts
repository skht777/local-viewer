// ファイルブラウザーのキーボードショートカット
// - 矢印キー / WASD でグリッド内移動
// - g / Enter で進入、Space でビューワー表示
// - b で親ディレクトリ、t でツリーにフォーカス移動
// - m でモード切替、n / u でソート、1/2/3 でタブ切替
// - enabled で排他制御（ツリーフォーカス中は無効化）

import { useHotkeys } from "react-hotkeys-hook";
import type { ViewerTab } from "./useViewerParams";

export interface BrowseKeyboardCallbacks {
  move: (delta: number) => void;
  action: () => void;
  open: () => void;
  goParent: () => void;
  focusTree: () => void;
  toggleMode: () => void;
  sortName: () => void;
  sortDate: () => void;
  tabChange: (tab: ViewerTab) => void;
  getColumnCount: () => number;
}

export function useBrowseKeyboard(callbacks: BrowseKeyboardCallbacks, enabled: boolean): void {
  // 左右移動
  useHotkeys("right, d", () => callbacks.move(1), { preventDefault: true, enabled });
  useHotkeys("left, a", () => callbacks.move(-1), { preventDefault: true, enabled });

  // 上下移動（列数分スキップ）
  useHotkeys("down, s", () => callbacks.move(callbacks.getColumnCount()), {
    preventDefault: true,
    enabled,
  });
  useHotkeys("up, w", () => callbacks.move(-callbacks.getColumnCount()), {
    preventDefault: true,
    enabled,
  });

  // 進入（ディレクトリ/アーカイブ進入、画像→ビューワー）
  useHotkeys("g, enter", () => callbacks.action(), { preventDefault: true, enabled });

  // ビューワーで開く
  useHotkeys("space", () => callbacks.open(), { preventDefault: true, enabled });

  // 親ディレクトリに戻る
  useHotkeys("b, backspace", () => callbacks.goParent(), { preventDefault: true, enabled });

  // ツリーにフォーカス移動
  useHotkeys("t", () => callbacks.focusTree(), { enabled });

  // モード切替（CG ↔ マンガ）
  useHotkeys("m", () => callbacks.toggleMode(), { enabled });

  // ソートトグル
  useHotkeys("n", () => callbacks.sortName(), { enabled });
  useHotkeys("u", () => callbacks.sortDate(), { enabled });

  // タブ切替
  useHotkeys("1", () => callbacks.tabChange("filesets"), { enabled });
  useHotkeys("2", () => callbacks.tabChange("images"), { enabled });
  useHotkeys("3", () => callbacks.tabChange("videos"), { enabled });
}
