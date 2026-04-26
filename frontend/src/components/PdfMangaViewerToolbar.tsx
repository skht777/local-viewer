// PdfMangaViewer 用のツールバーラッパー
// - デスクトップ: 自動表示/非表示
// - タッチ: 常時表示

import { MangaToolbar } from "./MangaToolbar";

interface PdfMangaViewerToolbarProps {
  isTouch: boolean;
  isToolbarVisible: boolean;
  currentIndex: number;
  totalCount: number;
  zoomLevel: number;
  scrollSpeed: number;
  pdfName: string;
  onScrollToImage: (index: number) => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomChange: (zoom: number) => void;
  onScrollSpeedChange: (speed: number) => void;
  onToggleFullscreen: () => void;
  onClose: () => void;
  onPrevSet: () => void;
  onNextSet: () => void;
  isSetJumpDisabled: boolean;
}

export function PdfMangaViewerToolbar(props: PdfMangaViewerToolbarProps) {
  const wrapperClass = props.isTouch
    ? "relative z-10"
    : `absolute top-0 right-0 left-0 z-10 transition-opacity duration-300 ${
        props.isToolbarVisible ? "opacity-100" : "pointer-events-none opacity-0"
      }`;
  return (
    <div data-testid="toolbar-wrapper" className={wrapperClass}>
      <MangaToolbar
        currentIndex={props.currentIndex}
        totalCount={props.totalCount}
        zoomLevel={props.zoomLevel}
        scrollSpeed={props.scrollSpeed}
        setName={props.pdfName}
        onScrollToImage={props.onScrollToImage}
        onZoomIn={props.onZoomIn}
        onZoomOut={props.onZoomOut}
        onZoomChange={props.onZoomChange}
        onScrollSpeedChange={props.onScrollSpeedChange}
        onToggleFullscreen={props.onToggleFullscreen}
        onClose={props.onClose}
        onPrevSet={props.onPrevSet}
        onNextSet={props.onNextSet}
        isSetJumpDisabled={props.isSetJumpDisabled}
      />
    </div>
  );
}
