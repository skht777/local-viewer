// スクロール位置からビューポート中央の画像 index を検出する
// - isProgrammaticScroll ref でプログラムスクロール中は位置由来の index 更新を停止する。
//   behavior:"smooth" のジャンプは着地までに数百 ms かかるため、途中経過の位置から
//   index を再計算するとスライダーのサムが目標値と現在値の間で跳ねる
// - 抑制の解除条件は次のいずれか:
//   1. "scrollend"（目標位置への着地）
//   2. ユーザー操作の割り込み（wheel / touchstart / キーボードスクロール）→ 即座に追従へ戻す
//   3. フォールバックのタイムアウト（"scrollend" 非対応や、そもそもスクロールが
//      発生しないケースでフラグが固着するのを防ぐ）
// - requestAnimationFrame でデバウンスして高頻度更新を防止
// - scrollToIndex でサムネイルクリック時のジャンプを提供

import { useCallback, useEffect, useRef, useState } from "react";
import type { Virtualizer } from "@tanstack/react-virtual";

const DEFAULT_SCROLL_AMOUNT = 200;
// 抑制フラグのフォールバック解除上限。smooth スクロールの実測（数百 ms）に余裕を持たせる
const PROGRAMMATIC_SCROLL_TIMEOUT_MS = 1000;

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
  const releaseTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  // プログラムスクロールの抑制を解除する（フォールバックタイマーも同時に破棄）
  const endProgrammaticScroll = useCallback(() => {
    isProgrammaticScroll.current = false;
    clearTimeout(releaseTimerRef.current);
    releaseTimerRef.current = undefined;
  }, []);

  // アンマウント時にフォールバックタイマーを破棄する
  useEffect(() => () => clearTimeout(releaseTimerRef.current), []);

  // スクロール位置からビューポート中央の画像 index を検出
  useEffect(() => {
    if (!scrollElement) {
      return;
    }

    const handleScroll = () => {
      // プログラムスクロール中は途中経過の位置による index 上書きをスキップ
      if (isProgrammaticScroll.current) {
        return;
      }

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

    // 目標位置への着地で抑制を解除
    const handleScrollEnd = () => {
      endProgrammaticScroll();
    };

    // ユーザー操作の割り込み: 抑制を即座に解除して現在位置への追従に戻す
    const handleUserScrollIntent = () => {
      if (!isProgrammaticScroll.current) {
        return;
      }
      endProgrammaticScroll();
      handleScroll();
    };

    scrollElement.addEventListener("scroll", handleScroll, { passive: true });
    scrollElement.addEventListener("scrollend", handleScrollEnd);
    scrollElement.addEventListener("wheel", handleUserScrollIntent, { passive: true });
    scrollElement.addEventListener("touchstart", handleUserScrollIntent, { passive: true });
    return () => {
      scrollElement.removeEventListener("scroll", handleScroll);
      scrollElement.removeEventListener("scrollend", handleScrollEnd);
      scrollElement.removeEventListener("wheel", handleUserScrollIntent);
      scrollElement.removeEventListener("touchstart", handleUserScrollIntent);
      cancelAnimationFrame(rafId.current);
    };
  }, [scrollElement, virtualizer, endProgrammaticScroll]);

  // プログラムスクロール共通: 抑制フラグを立てて scroll + index を即時確定する
  // index を同期的に確定させることでスライダーのサムが目標値に即座に合い、
  // smooth スクロールの着地までは handleScroll による上書きを抑制する
  const programmaticScrollTo = useCallback(
    (index: number, scrollFn: () => void) => {
      isProgrammaticScroll.current = true;
      clearTimeout(releaseTimerRef.current);
      scrollFn();
      setCurrentIndex(index);
      releaseTimerRef.current = setTimeout(endProgrammaticScroll, PROGRAMMATIC_SCROLL_TIMEOUT_MS);
    },
    [endProgrammaticScroll],
  );

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
      if (scrollElement) {
        scrollElement.scrollTop = 0;
      }
    });
  }, [scrollElement, programmaticScrollTo]);

  // End: virtualizer.scrollToIndex で末尾にジャンプ
  const scrollToBottom = useCallback(() => {
    programmaticScrollTo(totalCount - 1, () => {
      virtualizer.scrollToIndex(totalCount - 1, { align: "end", behavior: "instant" });
    });
  }, [virtualizer, totalCount, programmaticScrollTo]);

  // キーボードスクロール（scrollSpeed 適用）
  // ユーザー操作なので、進行中のプログラムスクロール抑制は解除して追従に戻す
  const scrollDown = useCallback(
    (amount = DEFAULT_SCROLL_AMOUNT) => {
      endProgrammaticScroll();
      scrollElement?.scrollBy(0, amount * scrollSpeed);
    },
    [scrollElement, scrollSpeed, endProgrammaticScroll],
  );

  const scrollUp = useCallback(
    (amount = DEFAULT_SCROLL_AMOUNT) => {
      endProgrammaticScroll();
      scrollElement?.scrollBy(0, -amount * scrollSpeed);
    },
    [scrollElement, scrollSpeed, endProgrammaticScroll],
  );

  return { currentIndex, scrollToImage, scrollToTop, scrollToBottom, scrollUp, scrollDown };
}
