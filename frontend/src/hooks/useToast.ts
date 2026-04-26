// トースト表示の状態管理
// - showToast(message) で表示、デフォルト2秒後に自動消去
// - showToast(message, durationOverride) で個別に表示時間を指定可能
//   （タイトルポップアップなど少し長く出したい用途）
// - toastDuration は現在表示中メッセージの実効 duration を返す。
//   <Toast duration={toastDuration} /> に渡して二重タイマーを同期させる
// - 連続呼び出しでタイマーリセット

import { useCallback, useRef, useState } from "react";

interface UseToastReturn {
  toastMessage: string | null;
  toastDuration: number;
  showToast: (message: string, durationOverride?: number) => void;
  dismissToast: () => void;
}

export function useToast(duration = 2000): UseToastReturn {
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const [toastDuration, setToastDuration] = useState<number>(duration);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const dismissToast = useCallback(() => {
    clearTimeout(timerRef.current);
    setToastMessage(null);
  }, []);

  const showToast = useCallback(
    (message: string, durationOverride?: number) => {
      const effectiveDuration = durationOverride ?? duration;
      clearTimeout(timerRef.current);
      setToastMessage(message);
      setToastDuration(effectiveDuration);
      timerRef.current = setTimeout(() => {
        setToastMessage(null);
      }, effectiveDuration);
    },
    [duration],
  );

  return { toastMessage, toastDuration, showToast, dismissToast };
}
