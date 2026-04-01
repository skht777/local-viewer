// スライダーのサム付近にページ番号を表示する吹き出し
// - 水平スライダー: サムの上に配置、下向き矢印
// - 縦スライダー: サムの左に配置、右向き矢印
// - visible で表示/非表示をフェードで切替

interface SliderTooltipProps {
  currentIndex: number;
  totalCount: number;
  /** サム位置 (px) */
  position: number;
  orientation: "horizontal" | "vertical";
  visible: boolean;
}

export function SliderTooltip({
  currentIndex,
  totalCount,
  position,
  orientation,
  visible,
}: SliderTooltipProps) {
  const label = `${currentIndex + 1} / ${totalCount}`;
  const isHorizontal = orientation === "horizontal";

  return (
    <div
      data-testid="slider-tooltip"
      className={`absolute flex flex-col items-center transition-opacity duration-200 ${
        visible ? "opacity-100" : "pointer-events-none opacity-0"
      } ${isHorizontal ? "-translate-x-1/2" : "-translate-x-full -translate-y-1/2"}`}
      style={
        isHorizontal
          ? { left: `${position}px`, bottom: "100%" }
          : { top: `${position}px`, right: "100%" }
      }
    >
      {isHorizontal ? (
        <>
          <span className="mb-1 whitespace-nowrap rounded bg-surface-overlay px-2 py-1 text-sm font-mono tabular-nums text-white">
            {label}
          </span>
          {/* 下向き矢印 */}
          <div className="h-0 w-0 border-x-4 border-t-4 border-x-transparent border-t-surface-overlay" />
        </>
      ) : (
        <span className="mr-2 whitespace-nowrap rounded bg-surface-overlay px-2 py-1 text-sm font-mono tabular-nums text-white">
          {label}
        </span>
      )}
    </div>
  );
}
