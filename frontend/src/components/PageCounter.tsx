// ページカウンター: "[セット名] 3 / 12" or "[セット名] 3-4 / 12" (見開き時)
// - ビューワー下部に配置

interface PageCounterProps {
  setName: string;
  current: number;
  currentEnd?: number;
  total: number;
}

export function PageCounter({ setName, current, currentEnd, total }: PageCounterProps) {
  const pageRange =
    currentEnd && currentEnd !== current ? `${current}-${currentEnd}` : `${current}`;
  const label = setName ? `${setName} ${pageRange} / ${total}` : `${pageRange} / ${total}`;

  return (
    <div
      data-testid="page-counter"
      className="pointer-events-none absolute bottom-4 left-1/2 -translate-x-1/2 rounded bg-black/60 px-3 py-1 text-sm text-white"
    >
      {label}
    </div>
  );
}
