// トースト表示の状態管理
// - showToast(message) で表示、デフォルト2秒後に自動消去
// - showToast(message, durationOverride) で個別に表示時間を指定可能
//   （タイトルポップアップなど少し長く出したい用途）
// - 表示時間のタイマーは本フックが単独で管理する。<Toast> は純粋な表示専用で、
//   タイマーを二重に持つと同一メッセージの再表示で旧タイマーが生き残り早期に消える
// - 連続呼び出しでタイマーリセット

import { useCallback, useEffect, useRef, useState } from "react";

interface UseToastReturn {
  toastMessage: string | null;
  showToast: (message: string, durationOverride?: number) => void;
  dismissToast: () => void;
}

export function useToast(duration = 2000): UseToastReturn {
  const [toastMessage, setToastMessage] = useState<string | null>(null);
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
      timerRef.current = setTimeout(() => {
        setToastMessage(null);
      }, effectiveDuration);
    },
    [duration],
  );

  // アンマウント時に自動消去タイマーを破棄する
  // （残存タイマーが unmount 後に発火して不要な setState を起こすのを防ぐ）
  useEffect(() => () => clearTimeout(timerRef.current), []);

  return { toastMessage, showToast, dismissToast };
}
