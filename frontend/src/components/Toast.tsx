// 自動消去トースト通知
// - 画面下部にメッセージを表示、ボタンなし
// - デフォルト2秒で自動消去（onDismiss を呼ぶ）

import { useEffect } from "react";

interface ToastProps {
  message: string;
  onDismiss: () => void;
  duration?: number;
}

export function Toast({ message, onDismiss, duration = 2000 }: ToastProps) {
  useEffect(() => {
    const timer = setTimeout(onDismiss, duration);
    return () => clearTimeout(timer);
  }, [onDismiss, duration]);

  return (
    <div
      data-testid="viewer-toast"
      className="fixed bottom-8 left-1/2 z-40 -translate-x-1/2 rounded-lg bg-surface-raised px-6 py-3 shadow-lg"
    >
      <p className="text-sm text-white">{message}</p>
    </div>
  );
}
