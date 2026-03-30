// セット移動確認ダイアログ
// - 画面下部にトースト風の確認 UI
// - Y/Enter で確認、N/Escape でキャンセル
// - 5秒で自動消去（onCancel を呼ぶ）
// - マウスホバー中はタイマーを一時停止

import { useCallback, useEffect, useRef } from "react";

interface NavigationPromptProps {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
  extraConfirmKeys?: string[];
}

export function NavigationPrompt({
  message,
  onConfirm,
  onCancel,
  extraConfirmKeys,
}: NavigationPromptProps) {
  // 5秒で自動消去（ホバー中は一時停止）
  const remainingRef = useRef(5000);
  const startTimeRef = useRef(Date.now());
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const startTimer = useCallback(() => {
    startTimeRef.current = Date.now();
    timerRef.current = setTimeout(onCancel, remainingRef.current);
  }, [onCancel]);

  const pauseTimer = useCallback(() => {
    clearTimeout(timerRef.current);
    const elapsed = Date.now() - startTimeRef.current;
    remainingRef.current = Math.max(0, remainingRef.current - elapsed);
  }, []);

  useEffect(() => {
    startTimer();
    return () => clearTimeout(timerRef.current);
  }, [startTimer]);

  // Y/Enter/extraConfirmKeys で確認、N でキャンセル
  // Escape は CgViewer/MangaViewer の handleEscape チェーンに任せる（二重呼び出し回避）
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (key === "y" || e.key === "Enter" || extraConfirmKeys?.includes(key)) {
        e.preventDefault();
        onConfirm();
      } else if (key === "n") {
        e.preventDefault();
        onCancel();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onConfirm, onCancel, extraConfirmKeys]);

  return (
    <div
      data-testid="navigation-prompt"
      className="fixed bottom-8 left-1/2 z-50 -translate-x-1/2 rounded-lg bg-surface-raised px-6 py-3 shadow-lg"
      onMouseEnter={pauseTimer}
      onMouseLeave={startTimer}
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
        <span>
          {extraConfirmKeys
            ? `${extraConfirmKeys.map((k) => k.toUpperCase()).join(" / ")} / Y / Enter で移動、N / Esc でキャンセル`
            : "Y / Enter で移動、N / Esc でキャンセル"}
        </span>
      </div>
    </div>
  );
}
