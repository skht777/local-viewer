// セット移動確認ダイアログ
// - 画面下部にトースト風の確認 UI
// - Y/Enter で確認、N/Escape でキャンセル
// - 5秒で自動消去（onCancel を呼ぶ）

import { useEffect } from "react";

interface NavigationPromptProps {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function NavigationPrompt({ message, onConfirm, onCancel }: NavigationPromptProps) {
  // 5秒で自動消去
  useEffect(() => {
    const timer = setTimeout(onCancel, 5000);
    return () => clearTimeout(timer);
  }, [onCancel]);

  // Y/Enter で確認、N でキャンセル
  // Escape は CgViewer/MangaViewer の handleEscape チェーンに任せる（二重呼び出し回避）
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "y" || e.key === "Y" || e.key === "Enter") {
        e.preventDefault();
        onConfirm();
      } else if (e.key === "n" || e.key === "N") {
        e.preventDefault();
        onCancel();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onConfirm, onCancel]);

  return (
    <div
      data-testid="navigation-prompt"
      className="fixed bottom-8 left-1/2 z-50 -translate-x-1/2 rounded-lg bg-surface-raised px-6 py-3 shadow-lg"
    >
      <p className="mb-2 text-sm text-white">{message}</p>
      <div className="flex items-center gap-3 text-xs text-gray-400">
        <button
          type="button"
          onClick={onConfirm}
          className="rounded bg-blue-600 px-3 py-1 text-white hover:bg-blue-500"
          aria-label="はい"
        >
          はい
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded bg-surface-raised px-3 py-1 text-gray-300 hover:bg-surface-overlay"
          aria-label="いいえ"
        >
          いいえ
        </button>
        <span>Y / Enter で移動、N / Esc でキャンセル</span>
      </div>
    </div>
  );
}
