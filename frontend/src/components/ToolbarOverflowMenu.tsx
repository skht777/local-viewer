// ツールバーのオーバーフロー (⋯) メニュー
// - details/summary ベースの軽量 popover (state 管理なし、Tailwind v4 のみ)
// - メニュー項目クリックで details の open を閉じる
// - モバイルでツールバーに収まりきらない優先度低要素 (Home/End/Help 等) を集約
//
// アイテムは MenuItem[] で渡す。各 onClick 後に popover は自動的に閉じる。

import { useCallback } from "react";

export interface OverflowMenuItem {
  label: string;
  onClick: () => void;
  "data-testid"?: string;
}

interface ToolbarOverflowMenuProps {
  items: OverflowMenuItem[];
  ariaLabel?: string;
  className?: string;
}

export function ToolbarOverflowMenu({
  items,
  ariaLabel = "その他のメニュー",
  className,
}: ToolbarOverflowMenuProps) {
  // クリックされた要素から祖先 details を閉じるユーティリティ
  // (各 onClick で popover を畳むため、項目選択後に手動で閉じる必要がない)
  const handleItemClick = useCallback((onClick: () => void) => {
    return (e: React.MouseEvent<HTMLButtonElement>) => {
      const details = e.currentTarget.closest("details");
      if (details) {
        details.open = false;
      }
      onClick();
    };
  }, []);

  return (
    <details className={`relative ${className ?? ""}`} data-testid="toolbar-overflow">
      <summary
        aria-label={ariaLabel}
        className="cursor-pointer list-none rounded px-3 py-2 text-sm text-gray-300 hover:bg-surface-raised [&::-webkit-details-marker]:hidden"
        data-testid="toolbar-overflow-summary"
      >
        ⋯
      </summary>
      <div
        role="menu"
        // z-40: 親 toolbar-wrapper (z-30) より上、KeyboardHelp (z-60) より下
        // max-w で safe-area 含む画面外オーバーフローを保護 (右端からの余白 16px)
        className="absolute right-0 z-40 mt-1 min-w-[10rem] max-w-[calc(100vw-2rem)] rounded-lg bg-surface-raised p-1 shadow-xl ring-1 ring-white/10"
      >
        {items.map((item) => (
          <button
            key={item.label}
            type="button"
            onClick={handleItemClick(item.onClick)}
            data-testid={item["data-testid"]}
            className="block w-full rounded px-3 py-2.5 text-left text-sm text-gray-300 hover:bg-surface-overlay"
            role="menuitem"
          >
            {item.label}
          </button>
        ))}
      </div>
    </details>
  );
}
