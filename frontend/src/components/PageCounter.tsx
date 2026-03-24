// ページカウンター: "[セット名] 3 / 12" 形式で表示
// - ビューワー下部に配置

interface PageCounterProps {
  setName: string;
  current: number;
  total: number;
}

export function PageCounter({ setName, current, total }: PageCounterProps) {
  const label = setName ? `${setName} ${current} / ${total}` : `${current} / ${total}`;

  return (
    <div className="pointer-events-none absolute bottom-4 left-1/2 -translate-x-1/2 rounded bg-black/60 px-3 py-1 text-sm text-white">
      {label}
    </div>
  );
}
