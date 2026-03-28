// マンガモード本体: 全画像を縦スクロールで表示
// - @tanstack/react-virtual で仮想スクロール + 遅延読み込み
// - zoomLevel に応じた画像幅制御（コンテナ幅 * zoomLevel / 100）
// - スクロール位置からページ番号を自動検出
// - ズーム変更時はスクロールアンカーを維持
// - カーソルオートハイド（1秒 idle → cursor: none）
// - セット間ジャンプ（useSetJump）+ NavigationPrompt

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { BrowseEntry } from "../types/api";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useMangaScroll } from "../hooks/useMangaScroll";
import { useMangaKeyboard } from "../hooks/useMangaKeyboard";
import { useSetJump } from "../hooks/useSetJump";
import type { ViewerMode } from "../hooks/useViewerParams";
import { MangaToolbar } from "./MangaToolbar";
import { NavigationPrompt } from "./NavigationPrompt";
import { PageCounter } from "./PageCounter";
import { ThumbnailSidebar } from "./ThumbnailSidebar";

interface MangaViewerProps {
  images: BrowseEntry[];
  currentIndex: number;
  setName: string;
  parentNodeId: string | null;
  currentNodeId: string | null;
  mode: ViewerMode;
  onIndexChange: (index: number) => void;
  onModeChange: (mode: ViewerMode) => void;
  onClose: () => void;
}

export function MangaViewer({
  images,
  currentIndex,
  setName,
  parentNodeId,
  currentNodeId,
  mode,
  onIndexChange,
  onModeChange,
  onClose,
}: MangaViewerProps) {
  const zoomLevel = useViewerStore((s) => s.zoomLevel);
  const setZoomLevel = useViewerStore((s) => s.setZoomLevel);
  const zoomIn = useViewerStore((s) => s.zoomIn);
  const zoomOut = useViewerStore((s) => s.zoomOut);
  const scrollSpeed = useViewerStore((s) => s.scrollSpeed);
  const setScrollSpeed = useViewerStore((s) => s.setScrollSpeed);
  const isSidebarOpen = useViewerStore((s) => s.isSidebarOpen);
  const { isFullscreen, toggleFullscreen } = useFullscreen();

  // スクロールコンテナ
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);
  useEffect(() => {
    setScrollElement(scrollRef.current);
  }, []);

  // 仮想スクロール（estimateSize: 3:4 縦長推定）
  const virtualizer = useVirtualizer({
    count: images.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => {
      const containerWidth = scrollRef.current?.clientWidth ?? 800;
      return (containerWidth * zoomLevel * 4) / (100 * 3);
    },
    overscan: 3,
  });

  // スクロール位置からのページ検出 + スクロール操作
  const mangaScroll = useMangaScroll({
    virtualizer,
    scrollElement,
    totalCount: images.length,
    scrollSpeed,
  });

  // currentIndex を URL に同期
  useEffect(() => {
    if (mangaScroll.currentIndex !== currentIndex) {
      onIndexChange(mangaScroll.currentIndex);
    }
  }, [mangaScroll.currentIndex, currentIndex, onIndexChange]);

  // 初期表示で currentIndex の位置にスクロール（モード切替時の位置引き継ぎ）
  const initialScrollDone = useRef(false);
  useEffect(() => {
    if (!initialScrollDone.current && currentIndex > 0 && images.length > 0) {
      virtualizer.scrollToIndex(currentIndex, { align: "start" });
      initialScrollDone.current = true;
    }
  }, [currentIndex, images.length, virtualizer]);

  // ズーム変更時: スクロールアンカー維持
  const prevZoomLevel = useRef(zoomLevel);
  useEffect(() => {
    if (prevZoomLevel.current !== zoomLevel) {
      const anchorIndex = mangaScroll.currentIndex;
      virtualizer.measure();
      // 次フレームで scrollToIndex（measure 反映後）
      requestAnimationFrame(() => {
        virtualizer.scrollToIndex(anchorIndex, { align: "start" });
      });
      prevZoomLevel.current = zoomLevel;
    }
  }, [zoomLevel, virtualizer, mangaScroll.currentIndex]);

  // セット間ジャンプ
  const setJump = useSetJump({ currentNodeId, parentNodeId, mode });

  // Escape 優先順位: (1) プロンプト閉じ → (2) フルスクリーン解除 → (3) ビューワー閉じ
  const handleEscape = useCallback(() => {
    if (setJump.prompt) {
      setJump.dismissPrompt();
      return;
    }
    if (isFullscreen) {
      document.exitFullscreen();
      return;
    }
    onClose();
  }, [setJump, isFullscreen, onClose]);

  // キーボードショートカット
  useMangaKeyboard({
    scrollUp: mangaScroll.scrollUp,
    scrollDown: mangaScroll.scrollDown,
    scrollToTop: mangaScroll.scrollToTop,
    scrollToBottom: mangaScroll.scrollToBottom,
    onEscape: handleEscape,
    toggleFullscreen,
    toggleSidebar: useViewerStore((s) => s.toggleSidebar),
    toggleMode: () => onModeChange(mode === "manga" ? "cg" : "manga"),
    goNextSet: setJump.goNextSet,
    goPrevSet: setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
    zoomIn,
    zoomOut,
    zoomReset: () => setZoomLevel(100),
  });

  // カーソルオートハイド
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const handleMouseMove = useCallback(() => {
    if (scrollRef.current) {
      scrollRef.current.style.cursor = "";
    }
    clearTimeout(cursorTimerRef.current);
    cursorTimerRef.current = setTimeout(() => {
      if (scrollRef.current) {
        scrollRef.current.style.cursor = "none";
      }
    }, 1000);
  }, []);

  // 画像幅（コンテナ幅 * zoomLevel / 100）
  const imageWidth = `${zoomLevel}%`;

  return (
    <div data-testid="manga-viewer" className="fixed inset-0 z-50 flex bg-black">
      {/* サムネイルサイドバー（instant 追従で jank 防止） */}
      {isSidebarOpen && (
        <ThumbnailSidebar
          images={images}
          currentIndex={mangaScroll.currentIndex}
          onSelect={mangaScroll.scrollToImage}
          scrollBehavior="instant"
        />
      )}

      {/* メインエリア */}
      <div className="relative flex flex-1 flex-col overflow-hidden">
        {/* ツールバー */}
        <MangaToolbar
          currentIndex={mangaScroll.currentIndex}
          totalCount={images.length}
          zoomLevel={zoomLevel}
          scrollSpeed={scrollSpeed}
          onScrollToImage={mangaScroll.scrollToImage}
          onZoomIn={zoomIn}
          onZoomOut={zoomOut}
          onZoomChange={setZoomLevel}
          onScrollSpeedChange={setScrollSpeed}
          onToggleFullscreen={toggleFullscreen}
          onClose={onClose}
        />

        {/* 仮想スクロール画像リスト */}
        <div
          ref={scrollRef}
          data-testid="manga-scroll-area"
          className="flex-1 overflow-auto"
          onMouseMove={handleMouseMove}
        >
          <div
            className="relative mx-auto"
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: imageWidth,
            }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const entry = images[virtualRow.index];
              return (
                <div
                  key={entry.node_id}
                  ref={virtualizer.measureElement}
                  data-index={virtualRow.index}
                  className="absolute left-0 w-full"
                  style={{ top: `${virtualRow.start}px` }}
                >
                  <img
                    src={`/api/file/${entry.node_id}`}
                    alt={entry.name}
                    className="w-full"
                    loading="lazy"
                    decoding="async"
                  />
                </div>
              );
            })}
          </div>
        </div>

        {/* ページカウンター */}
        <PageCounter
          setName={setName}
          current={mangaScroll.currentIndex + 1}
          total={images.length}
        />

        {/* セット間ジャンプの確認プロンプト */}
        {setJump.prompt && (
          <NavigationPrompt
            message={setJump.prompt.message}
            onConfirm={setJump.prompt.onConfirm}
            onCancel={setJump.prompt.onCancel}
          />
        )}
      </div>
    </div>
  );
}
