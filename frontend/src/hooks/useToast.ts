// トースト表示の状態管理
// - showToast(message) で表示、2秒後に自動消去
// - 連続呼び出しでタイマーリセット

import { useCallback, useRef, useState } from "react";

interface UseToastReturn {
  toastMessage: string | null;
  showToast: (message: string) => void;
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
    (message: string) => {
      clearTimeout(timerRef.current);
      setToastMessage(message);
      timerRef.current = setTimeout(() => {
        setToastMessage(null);
      }, duration);
    },
    [duration],
  );

  return { toastMessage, showToast, dismissToast };
}
