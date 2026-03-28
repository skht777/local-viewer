// スクロール位置からビューポート中央の画像 index を検出する
// - isProgrammaticScroll ref でプログラムスクロール中は URL 同期を停止
// - requestAnimationFrame でデバウンスして高頻度更新を防止
// - scrollToIndex でサムネイルクリック時のジャンプを提供

import { useCallback, useEffect, useRef, useState } from "react";
import type { Virtualizer } from "@tanstack/react-virtual";

const DEFAULT_SCROLL_AMOUNT = 200;

interface UseMangaScrollProps {
  virtualizer: Virtualizer<HTMLDivElement, Element>;
  scrollElement: HTMLDivElement | null;
  totalCount: number;
  scrollSpeed: number;
}

interface UseMangaScrollReturn {
  currentIndex: number;
  scrollToImage: (index: number) => void;
  scrollToTop: () => void;
  scrollToBottom: () => void;
  scrollUp: (amount?: number) => void;
  scrollDown: (amount?: number) => void;
}

export function useMangaScroll({
  virtualizer,
  scrollElement,
  totalCount,
  scrollSpeed,
}: UseMangaScrollProps): UseMangaScrollReturn {
  const [currentIndex, setCurrentIndex] = useState(0);
  const isProgrammaticScroll = useRef(false);
  const rafId = useRef(0);

  // スクロール位置からビューポート中央の画像 index を検出
  useEffect(() => {
    if (!scrollElement) return;

    const handleScroll = () => {
      // プログラムスクロール中は URL 同期をスキップ
      if (isProgrammaticScroll.current) return;

      cancelAnimationFrame(rafId.current);
      rafId.current = requestAnimationFrame(() => {
        const viewportCenter = scrollElement.scrollTop + scrollElement.clientHeight / 2;
        const items = virtualizer.getVirtualItems();
        for (const item of items) {
          if (item.start <= viewportCenter && viewportCenter < item.start + item.size) {
            setCurrentIndex(item.index);
            return;
          }
        }
        // フォールバック: 最後の表示アイテム
        if (items.length > 0) {
          setCurrentIndex(items[items.length - 1].index);
        }
      });
    };

    scrollElement.addEventListener("scroll", handleScroll, { passive: true });
    return () => {
      scrollElement.removeEventListener("scroll", handleScroll);
      cancelAnimationFrame(rafId.current);
    };
  }, [scrollElement, virtualizer]);

  // プログラムスクロール共通: scroll + index 設定 + フラグ管理
  // isProgrammaticScroll は setCurrentIndex の React 再レンダー後に解除して
  // handleScroll による上書きを防ぐ
  const programmaticScrollTo = useCallback((index: number, scrollFn: () => void) => {
    isProgrammaticScroll.current = true;
    scrollFn();
    requestAnimationFrame(() => {
      setCurrentIndex(index);
      requestAnimationFrame(() => {
        isProgrammaticScroll.current = false;
      });
    });
  }, []);

  // ページセレクト/サムネイルクリック: virtualizer で指定ページにスクロール
  const scrollToImage = useCallback(
    (index: number) => {
      programmaticScrollTo(index, () => {
        virtualizer.scrollToIndex(index, { align: "start", behavior: "smooth" });
      });
    },
    [virtualizer, programmaticScrollTo],
  );

  // Home: DOM scrollTop=0 で即座にジャンプ
  const scrollToTop = useCallback(() => {
    programmaticScrollTo(0, () => {
      if (scrollElement) scrollElement.scrollTop = 0;
    });
  }, [scrollElement, programmaticScrollTo]);

  // End: virtualizer.scrollToIndex で末尾にジャンプ
  const scrollToBottom = useCallback(() => {
    programmaticScrollTo(totalCount - 1, () => {
      virtualizer.scrollToIndex(totalCount - 1, { align: "end", behavior: "instant" });
    });
  }, [virtualizer, totalCount, programmaticScrollTo]);

  // キーボードスクロール（scrollSpeed 適用）
  const scrollDown = useCallback(
    (amount = DEFAULT_SCROLL_AMOUNT) => {
      scrollElement?.scrollBy(0, amount * scrollSpeed);
    },
    [scrollElement, scrollSpeed],
  );

  const scrollUp = useCallback(
    (amount = DEFAULT_SCROLL_AMOUNT) => {
      scrollElement?.scrollBy(0, -amount * scrollSpeed);
    },
    [scrollElement, scrollSpeed],
  );

  return { currentIndex, scrollToImage, scrollToTop, scrollToBottom, scrollUp, scrollDown };
}
