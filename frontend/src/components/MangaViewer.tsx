// マンガモード本体: 全画像を縦スクロールで表示
// - @tanstack/react-virtual で仮想スクロール + 遅延読み込み
// - zoomLevel に応じた画像幅制御（コンテナ幅 * zoomLevel / 100）
// - スクロール位置からページ番号を自動検出
// - ズーム変更時はスクロールアンカーを維持
// - カーソルオートハイド（1秒 idle → cursor: none）
// - セット間ジャンプ（useSetJump）+ NavigationPrompt

import { useCallback, useRef, useState } from "react";
import type { AncestorEntry, BrowseEntry } from "../types/api";
import { useViewerStore } from "../stores/viewerStore";
import { useCursorAutoHide } from "../hooks/useCursorAutoHide";
import { useFullscreen } from "../hooks/useFullscreen";
import { useMangaScroll } from "../hooks/useMangaScroll";
import { useMangaKeyboard } from "../hooks/useMangaKeyboard";
import { useMangaVirtualizer } from "../hooks/useMangaVirtualizer";
import { useSetJump } from "../hooks/useSetJump";
import { useSiblingPrefetch } from "../hooks/useSiblingPrefetch";
import { useToast } from "../hooks/useToast";
import { useToolbarAutoHide } from "../hooks/useToolbarAutoHide";
import { useUrlIndexSync } from "../hooks/useUrlIndexSync";
import type { SortOrder, ViewerMode } from "../hooks/useViewerParams";
import { formatPageLabel } from "../utils/formatPageLabel";
import { KeyboardHelp, MANGA_SHORTCUTS } from "./KeyboardHelp";
import { MangaToolbar } from "./MangaToolbar";
import { NavigationPrompt } from "./NavigationPrompt";
import { Toast } from "./Toast";
import { VerticalPageSlider } from "./VerticalPageSlider";

interface MangaViewerProps {
  images: BrowseEntry[];
  currentIndex: number;
  setName: string;
  parentNodeId: string | null;
  currentNodeId: string | null;
  ancestors?: AncestorEntry[];
  mode: ViewerMode;
  sort?: SortOrder;
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
  sort,
  onIndexChange,
  onClose,
}: MangaViewerProps) {
  const zoomLevel = useViewerStore((s) => s.zoomLevel);
  const setZoomLevel = useViewerStore((s) => s.setZoomLevel);
  const zoomIn = useViewerStore((s) => s.zoomIn);
  const zoomOut = useViewerStore((s) => s.zoomOut);
  const scrollSpeed = useViewerStore((s) => s.scrollSpeed);
  const setScrollSpeed = useViewerStore((s) => s.setScrollSpeed);
  const viewerTransitionId = useViewerStore((s) => s.viewerTransitionId);
  const { toggleFullscreen } = useFullscreen();

  // 仮想スクロール（estimateSize: 3:4 縦長推定）+ 初期スクロール + zoom anchor 維持
  const anchorIndexRef = useRef(0);
  const { virtualizer, scrollRef, scrollElement } = useMangaVirtualizer({
    count: images.length,
    zoomLevel,
    initialIndex: currentIndex,
    anchorIndexRef,
  });

  // スクロール位置からのページ検出 + スクロール操作
  const mangaScroll = useMangaScroll({
    virtualizer,
    scrollElement,
    totalCount: images.length,
    scrollSpeed,
  });

  // zoom anchor 用に最新 index を ref に反映（mangaScroll は virtualizer の後に確定）
  anchorIndexRef.current = mangaScroll.currentIndex;

  // currentIndex を URL に同期（debounce 200ms）
  // 初期マウント時の virtualizer 再計測・画像遅延ロードで起きるスクロール位置の揺らぎが
  // 毎フレームの setSearchParams 連鎖を誘発し React の update depth 制限（#185）を超える
  // ケースがあるため、揺らぎを吸収してから URL に反映する
  useUrlIndexSync({
    currentIndex: mangaScroll.currentIndex,
    externalIndex: currentIndex,
    onChange: onIndexChange,
    debounceMs: 200,
  });

  // ツールバー自動表示/非表示

  const { isToolbarVisible, isTouch, containerCallbackRef } = useToolbarAutoHide();

  // キーボードヘルプ
  const [isHelpOpen, setIsHelpOpen] = useState(false);

  // セット境界トースト（duration は useToast 内部 timer と Toast 側を同期）
  const { toastMessage, toastDuration, showToast, dismissToast } = useToast();

  // セット間ジャンプ + バックグラウンドプリフェッチ
  const setJump = useSetJump({
    currentNodeId,
    parentNodeId,
    ancestors,
    mode,
    sort,
    onBoundary: showToast,
  });
  useSiblingPrefetch({ currentNodeId, parentNodeId, ancestors, sort });

  // Escape: ダイアログ閉じのみ（ビューワー閉じは B キー）
  const handleEscape = useCallback(() => {
    if (isHelpOpen) {
      setIsHelpOpen(false);
      return;
    }
    if (setJump.prompt) {
      setJump.dismissPrompt();
    }
  }, [isHelpOpen, setJump]);

  // キーボードショートカット
  useMangaKeyboard({
    scrollUp: mangaScroll.scrollUp,
    scrollDown: mangaScroll.scrollDown,
    scrollToTop: mangaScroll.scrollToTop,
    scrollToBottom: mangaScroll.scrollToBottom,
    onEscape: handleEscape,
    onClose,
    toggleFullscreen,
    goNextSet: setJump.prompt ? undefined : setJump.goNextSet,
    goPrevSet: setJump.prompt ? undefined : setJump.goPrevSet,
    goNextSetParent: setJump.goNextSetParent,
    goPrevSetParent: setJump.goPrevSetParent,
    zoomIn,
    zoomOut,
    zoomReset: () => setZoomLevel(100),
    toggleHelp: () => setIsHelpOpen((prev) => !prev),
    // タイトル + 現在ページ / 総ページ を 3 秒トースト表示
    showTitle: () =>
      showToast(formatPageLabel(setName, mangaScroll.currentIndex + 1, images.length), 3000),
  });

  // カーソルオートハイド（1秒 idle で消す）
  const { resetCursorTimer } = useCursorAutoHide(scrollRef);

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
            onPrevSet={setJump.goPrevSet}
            onNextSet={setJump.goNextSet}
            isSetJumpDisabled={setJump.prompt != null || viewerTransitionId > 0}
          />
        </div>

        {/* 仮想スクロール画像リスト */}
        {/* scrollbar-gutter: stable でスクロールバー分を常時確保し、 */}
        {/* 画像ロード中の総高さ変化 → スクロールバー出現/消滅 → 画像幅変動 → */}
        {/* 総高さ変動の無限フィードバック（React #185）を防ぐ */}
        <div
          ref={scrollRef}
          data-testid="manga-scroll-area"
          className="flex-1 overflow-y-scroll"
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
              // min-height に virtualRow.size（初回は estimateSize、以降は実測）を設定し、
              // 画像ロード前の wrapper 高さを estimate で固定。画像ロード後は natural height
              // がそれを上回れば単方向に伸びるのみでオシレーションしない。
              // これと overflow-y-scroll の併用で Virtualizer の measureElement → onChange
              // → rerender 無限ループ（React #185）を防ぐ。
              return (
                <div
                  key={entry.node_id}
                  ref={virtualizer.measureElement}
                  data-index={virtualRow.index}
                  className="absolute left-0 w-full"
                  style={{ top: `${virtualRow.start}px`, minHeight: `${virtualRow.size}px` }}
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
        {/* セット境界トースト */}
        {toastMessage && (
          <Toast message={toastMessage} onDismiss={dismissToast} duration={toastDuration} />
        )}

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
