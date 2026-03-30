// キーボードショートカットヘルプパネル
// - ? キーで表示/非表示
// - Escape またはオーバーレイクリックで閉じる

import { Fragment } from "react";

export interface ShortcutEntry {
  key: string;
  description: string;
}

interface KeyboardHelpProps {
  shortcuts: ShortcutEntry[];
  onClose: () => void;
}

export const CG_SHORTCUTS: ShortcutEntry[] = [
  { key: "→ / D", description: "次のページ" },
  { key: "← / A", description: "前のページ" },
  { key: "↑ / W", description: "上にスクロール" },
  { key: "↓ / S", description: "下にスクロール" },
  { key: "Home", description: "最初のページ" },
  { key: "End", description: "最後のページ" },
  { key: "V", description: "幅フィット" },
  { key: "H", description: "高さフィット" },
  { key: "Q", description: "見開き切替" },
  { key: "F", description: "フルスクリーン" },
  { key: "X / PgDn", description: "次のセット" },
  { key: "Z / PgUp", description: "前のセット" },
  { key: "Shift+X", description: "次のセット（親）" },
  { key: "Shift+Z", description: "前のセット（親）" },
  { key: "Esc", description: "閉じる" },
  { key: "?", description: "ヘルプ表示/非表示" },
];

export const MANGA_SHORTCUTS: ShortcutEntry[] = [
  { key: "↑ / W", description: "上にスクロール" },
  { key: "↓ / S", description: "下にスクロール" },
  { key: "Home", description: "先頭へ" },
  { key: "End", description: "末尾へ" },
  { key: "+", description: "ズームイン" },
  { key: "-", description: "ズームアウト" },
  { key: "0", description: "ズームリセット" },
  { key: "F", description: "フルスクリーン" },
  { key: "X / PgDn", description: "次のセット" },
  { key: "Z / PgUp", description: "前のセット" },
  { key: "Shift+X", description: "次のセット（親）" },
  { key: "Shift+Z", description: "前のセット（親）" },
  { key: "Esc", description: "閉じる" },
  { key: "?", description: "ヘルプ表示/非表示" },
];

export function KeyboardHelp({ shortcuts, onClose }: KeyboardHelpProps) {
  return (
    <div
      data-testid="keyboard-help-overlay"
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/70"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="max-h-[80vh] w-full max-w-md overflow-y-auto rounded-xl bg-surface-card p-6 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-bold text-white">キーボードショートカット</h2>
          <button
            type="button"
            onClick={onClose}
            className="text-gray-400 hover:text-white"
            aria-label="閉じる"
          >
            ✕
          </button>
        </div>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
          {shortcuts.map((s) => (
            <Fragment key={s.key}>
              <dt className="text-right font-mono text-sm text-blue-400">{s.key}</dt>
              <dd className="text-sm text-gray-300">{s.description}</dd>
            </Fragment>
          ))}
        </dl>
        <p className="mt-4 text-center text-xs text-gray-500">? で閉じる</p>
      </div>
    </div>
  );
}
