// マンガモード本体: 全画像を縦スクロールで表示
// - @tanstack/react-virtual で仮想スクロール + 遅延読み込み
// - zoomLevel に応じた画像幅制御（コンテナ幅 * zoomLevel / 100）
// - スクロール位置からページ番号を自動検出
// - ズーム変更時はスクロールアンカーを維持
// - カーソルオートハイド（1秒 idle → cursor: none）
// - セット間ジャンプ（useSetJump）+ NavigationPrompt

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { AncestorEntry, BrowseEntry } from "../types/api";
import { useViewerStore } from "../stores/viewerStore";
import { useFullscreen } from "../hooks/useFullscreen";
import { useMangaScroll } from "../hooks/useMangaScroll";
import { useMangaKeyboard } from "../hooks/useMangaKeyboard";
import { useSetJump } from "../hooks/useSetJump";
import { useToolbarAutoHide } from "../hooks/useToolbarAutoHide";
import type { ViewerMode } from "../hooks/useViewerParams";
import { KeyboardHelp, MANGA_SHORTCUTS } from "./KeyboardHelp";
import { MangaToolbar } from "./MangaToolbar";
import { NavigationPrompt } from "./NavigationPrompt";
import { VerticalPageSlider } from "./VerticalPageSlider";

interface MangaViewerProps {
  images: BrowseEntry[];
  currentIndex: number;
  setName: string;
  parentNodeId: string | null;
  currentNodeId: string | null;
  ancestors?: AncestorEntry[];
  mode: ViewerMode;
  onIndexChange: (index: number) => void;
  onClose: () => void;
}

export function MangaViewer({
  images,
  currentIndex,
  setName,
  parentNodeId,
  currentNodeId,
  ancestors,
  mode,
  onIndexChange,
  onClose,
}: MangaViewerProps) {
  const zoomLevel = useViewerStore((s) => s.zoomLevel);
  const setZoomLevel = useViewerStore((s) => s.setZoomLevel);
  const zoomIn = useViewerStore((s) => s.zoomIn);
  const zoomOut = useViewerStore((s) => s.zoomOut);
  const scrollSpeed = useViewerStore((s) => s.scrollSpeed);
  const setScrollSpeed = useViewerStore((s) => s.setScrollSpeed);
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

  // ツールバー自動表示/非表示

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();

  // キーボードヘルプ
  const [isHelpOpen, setIsHelpOpen] = useState(false);

  // セット間ジャンプ
  const setJump = useSetJump({ currentNodeId, parentNodeId, ancestors, mode });

  // Escape 優先順位: (1) ヘルプ閉じ → (2) プロンプト閉じ → (3) フルスクリーン解除 → (4) ビューワー閉じ
  const handleEscape = useCallback(() => {
    if (isHelpOpen) {
      setIsHelpOpen(false);
      return;
    }
    if (setJump.prompt) {
      setJump.dismissPrompt();
      return;
    }
    if (isFullscreen) {
      document.exitFullscreen();
      return;
    }
    onClose();
  }, [isHelpOpen, setJump, isFullscreen, onClose]);

  // キーボードショートカット
  useMangaKeyboard({
    scrollUp: mangaScroll.scrollUp,
    scrollDown: mangaScroll.scrollDown,
    scrollToTop: mangaScroll.scrollToTop,
    scrollToBottom: mangaScroll.scrollToBottom,
    onEscape: handleEscape,
    toggleFullscreen,
    goNextSet: setJump.prompt ? undefined : setJump.goNextSet,
    goPrevSet: setJump.prompt ? undefined : setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
    zoomIn,
    zoomOut,
    zoomReset: () => setZoomLevel(100),
    toggleHelp: () => setIsHelpOpen((prev) => !prev),
  });

  // カーソルオートハイド
  const cursorTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  // カーソルオートハイドをリセット（スライダー操作時にも呼ばれる）
  const resetCursorTimer = useCallback(() => {
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
      {/* メインエリア */}
      <div ref={containerCallbackRef} className="relative flex flex-1 flex-col overflow-hidden">
        {/* ツールバー（デスクトップ: 自動表示/非表示、タッチ: 常時表示・通常フロー） */}
        <div
          data-testid="toolbar-wrapper"
          className={
            isTouch
              ? "relative z-10"
              : `absolute top-0 right-0 left-0 z-10 transition-opacity duration-300 ${isToolbarVisible ? "opacity-100" : "pointer-events-none opacity-0"}`
          }
        >
          <MangaToolbar
            currentIndex={mangaScroll.currentIndex}
            totalCount={images.length}
            zoomLevel={zoomLevel}
            scrollSpeed={scrollSpeed}
            setName={setName}
            onScrollToImage={mangaScroll.scrollToImage}
            onZoomIn={zoomIn}
            onZoomOut={zoomOut}
            onZoomChange={setZoomLevel}
            onScrollSpeedChange={setScrollSpeed}
            onToggleFullscreen={toggleFullscreen}
            onClose={onClose}
          />
        </div>

        {/* 仮想スクロール画像リスト */}
        <div
          ref={scrollRef}
          data-testid="manga-scroll-area"
          className="flex-1 overflow-auto"
          onMouseMove={resetCursorTimer}
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

        {/* ページスライダー（右端フェードイン） */}
        <VerticalPageSlider
          currentIndex={mangaScroll.currentIndex}
          totalCount={images.length}
          onGoTo={mangaScroll.scrollToImage}
          containerRef={scrollRef}
          onSliderActivity={resetCursorTimer}
        />

        {/* キーボードヘルプ */}
        {isHelpOpen && (
          <KeyboardHelp shortcuts={MANGA_SHORTCUTS} onClose={() => setIsHelpOpen(false)} />
        )}

        {/* セット間ジャンプの確認プロンプト */}
        {setJump.prompt && (
          <NavigationPrompt
            message={setJump.prompt.message}
            onConfirm={setJump.prompt.onConfirm}
            onCancel={setJump.prompt.onCancel}
            extraConfirmKeys={setJump.prompt.extraConfirmKeys}
          />
        )}
      </div>
    </div>
  );
}
