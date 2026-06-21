// PDF CG モードの次ページ先読み
// - 現在の見開きグループの「次グループ」のページをオフスクリーンで先行描画し、
//   描画キャッシュへ格納する。advance 時に同一 cacheKey でヒットして即時表示される
// - 連続ページ送り時の無駄描画を避けるため短い debounce を挟む
// - 戻り（前グループ）は PdfCanvas が描画時にキャッシュ済みのため対象外

import { useEffect } from "react";
import type { FitMode, SpreadMode } from "../stores/viewerStore";
import type { PDFDocumentProxy } from "../lib/pdfjs";
import type { PdfRenderCache } from "./usePdfRenderCache";
import { renderPdfPageToCache } from "../utils/pdfRender";
import { computeSpreadGroup } from "../utils/spreadLayout";

const PREFETCH_DEBOUNCE_MS = 150;

interface UsePdfPagePrefetchParams {
  document: PDFDocumentProxy | null;
  // 0-based 現在ページ
  currentPage: number;
  pageCount: number;
  spreadMode: SpreadMode;
  fitMode: FitMode;
  // PdfCanvas に渡すのと同一の 1 ページ分のコンテナ幅（見開き時は半幅）
  pageContainerWidth: number;
  containerHeight: number;
  renderCache: PdfRenderCache | undefined;
}

export function usePdfPagePrefetch({
  document,
  currentPage,
  pageCount,
  spreadMode,
  fitMode,
  pageContainerWidth,
  containerHeight,
  renderCache,
}: UsePdfPagePrefetchParams): void {
  useEffect(() => {
    if (!document || !renderCache || pageCount <= 0) {
      return;
    }
    // 次グループの先頭 index を求め、そのグループの全ページを先読み対象にする
    const current = computeSpreadGroup(currentPage, pageCount, spreadMode);
    if (current.nextStart === null) {
      return;
    }
    const nextGroup = computeSpreadGroup(current.nextStart, pageCount, spreadMode);
    const targets = nextGroup.indices;
    if (targets.length === 0) {
      return;
    }

    let cancelled = false;
    const isCancelled = () => cancelled;
    const timer = setTimeout(() => {
      for (const idx of targets) {
        void renderPdfPageToCache({
          document,
          pageNumber: idx + 1,
          fitMode,
          containerWidth: pageContainerWidth,
          containerHeight,
          cache: renderCache,
          isCancelled,
        });
      }
    }, PREFETCH_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [
    document,
    currentPage,
    pageCount,
    spreadMode,
    fitMode,
    pageContainerWidth,
    containerHeight,
    renderCache,
  ]);
}
