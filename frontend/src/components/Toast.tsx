// トースト通知（純粋な表示コンポーネント）
// - 画面下部にメッセージを表示、ボタンなし
// - 表示時間の管理は useToast が単独で担う。ここに独自タイマーを持つと、
//   同一メッセージ・同一 duration の再表示で effect の依存配列が変化せず
//   旧タイマーが生き残り、表示が早期に消える

interface ToastProps {
  message: string;
}

export function Toast({ message }: ToastProps) {
  return (
    <div
      data-testid="viewer-toast"
      className="fixed bottom-8 left-1/2 z-40 -translate-x-1/2 rounded-lg bg-surface-raised px-6 py-3 shadow-lg"
    >
      <p className="text-sm text-white">{message}</p>
    </div>
  );
}
