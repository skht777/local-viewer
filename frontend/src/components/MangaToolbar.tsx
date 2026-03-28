// マンガモードのツールバー
// - ページセレクト: <select> でページ直接ジャンプ
// - ズームスライダー: [-] ==O== 100% [+]
// - スクロール速度スライダー
// - フルスクリーン(F) + 閉じる

interface MangaToolbarProps {
  currentIndex: number;
  totalCount: number;
  zoomLevel: number;
  scrollSpeed: number;
  onScrollToImage: (index: number) => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomChange: (level: number) => void;
  onScrollSpeedChange: (speed: number) => void;
  onToggleFullscreen: () => void;
  onClose: () => void;
}

export function MangaToolbar({
  currentIndex,
  totalCount,
  zoomLevel,
  scrollSpeed,
  onScrollToImage,
  onZoomIn,
  onZoomOut,
  onZoomChange,
  onScrollSpeedChange,
  onToggleFullscreen,
  onClose,
}: MangaToolbarProps) {
  return (
    <div className="absolute top-0 right-0 left-0 z-10 flex items-center gap-2 bg-black/60 px-3 py-2">
      {/* ページセレクト */}
      <select
        value={currentIndex}
        onChange={(e) => onScrollToImage(Number(e.target.value))}
        className="rounded bg-gray-800 px-2 py-1 text-xs text-white"
        aria-label="ページ選択"
      >
        {Array.from({ length: totalCount }, (_, i) => (
          <option key={i} value={i}>
            Page {i + 1}
          </option>
        ))}
      </select>

      {/* ズームスライダー */}
      <button
        type="button"
        onClick={onZoomOut}
        className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
        aria-label="ズームアウト"
      >
        -
      </button>
      <input
        type="range"
        min={25}
        max={300}
        step={25}
        value={zoomLevel}
        onChange={(e) => onZoomChange(Number(e.target.value))}
        className="w-24"
        aria-label="ズーム"
      />
      <span
        className="min-w-[3rem] text-center text-xs text-gray-300"
        data-testid="manga-zoom-level"
      >
        {zoomLevel}%
      </span>
      <button
        type="button"
        onClick={onZoomIn}
        className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
        aria-label="ズームイン"
      >
        +
      </button>

      {/* スクロール速度スライダー */}
      <input
        type="range"
        min={0.5}
        max={3.0}
        step={0.5}
        value={scrollSpeed}
        onChange={(e) => onScrollSpeedChange(Number(e.target.value))}
        className="w-16"
        aria-label="スクロール速度"
      />
      <span className="text-xs text-gray-300" data-testid="manga-scroll-speed-label">
        {scrollSpeed}x
      </span>

      <div className="flex-1" />

      {/* フルスクリーン */}
      <button
        type="button"
        onClick={onToggleFullscreen}
        className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
        aria-label="フルスクリーン"
      >
        F
      </button>

      {/* 閉じる */}
      <button
        type="button"
        onClick={onClose}
        className="rounded px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
        aria-label="閉じる"
      >
        ✕
      </button>
    </div>
  );
}
