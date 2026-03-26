// PDF サムネイル blob URL を順次生成するフック
// - 各ページを小さい scale で canvas 描画 → blob URL 化
// - rAF ベースのバッチ処理で UI をブロックしない
// - currentIndex 近傍から優先生成
// - OffscreenCanvas / HTMLCanvasElement をフォールバック分岐
// - cleanup で blob URL を全て revokeObjectURL

import { useEffect, useRef, useState } from "react";
import type { PDFDocumentProxy } from "../lib/pdfjs";

interface UsePdfThumbnailsReturn {
  thumbnails: (string | null)[];
  isComplete: boolean;
}

const DEFAULT_THUMBNAIL_WIDTH = 80;
const BATCH_SIZE = 3;

// 近傍優先の生成順序を計算する
// currentIndex を中心に双方向に広げる
export function computeRenderOrder(pageCount: number, currentIndex: number): number[] {
  const order: number[] = [];
  const visited = new Set<number>();
  const center = Math.max(0, Math.min(currentIndex, pageCount - 1));

  for (let offset = 0; offset < pageCount; offset++) {
    for (const idx of [center + offset, center - offset]) {
      if (idx >= 0 && idx < pageCount && !visited.has(idx)) {
        visited.add(idx);
        order.push(idx);
      }
    }
  }
  return order;
}

// canvas を作成する (OffscreenCanvas フォールバック)
function createCanvas(width: number, height: number): HTMLCanvasElement | OffscreenCanvas {
  if (typeof OffscreenCanvas !== "undefined") {
    return new OffscreenCanvas(width, height);
  }
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
}

// canvas から blob URL を生成する
async function canvasToBlobUrl(canvas: HTMLCanvasElement | OffscreenCanvas): Promise<string> {
  if (canvas instanceof OffscreenCanvas) {
    const blob = await canvas.convertToBlob({ type: "image/png" });
    return URL.createObjectURL(blob);
  }
  return new Promise<string>((resolve) => {
    (canvas as HTMLCanvasElement).toBlob((blob) => {
      resolve(blob ? URL.createObjectURL(blob) : "");
    }, "image/png");
  });
}

export function usePdfThumbnails(
  document: PDFDocumentProxy | null,
  currentIndex = 0,
  thumbnailWidth = DEFAULT_THUMBNAIL_WIDTH,
): UsePdfThumbnailsReturn {
  const [thumbnails, setThumbnails] = useState<(string | null)[]>([]);
  const [isComplete, setIsComplete] = useState(true);
  const blobUrlsRef = useRef<string[]>([]);
  const cancelledRef = useRef(false);

  useEffect(() => {
    if (!document) {
      setThumbnails([]);
      setIsComplete(true);
      return;
    }

    const pageCount = document.numPages;
    cancelledRef.current = false;

    // 前回の blob URL を解放
    for (const url of blobUrlsRef.current) {
      URL.revokeObjectURL(url);
    }
    blobUrlsRef.current = [];

    // 初期化
    setThumbnails(new Array(pageCount).fill(null));
    setIsComplete(false);

    const renderOrder = computeRenderOrder(pageCount, currentIndex);
    let cursor = 0;

    // バッチ処理: BATCH_SIZE ページずつ setTimeout で分割 (UI ブロック回避)
    async function renderBatch() {
      if (cancelledRef.current || cursor >= renderOrder.length) {
        if (!cancelledRef.current) setIsComplete(true);
        return;
      }

      const batch = renderOrder.slice(cursor, cursor + BATCH_SIZE);
      cursor += BATCH_SIZE;

      for (const pageIdx of batch) {
        if (cancelledRef.current) return;

        try {
          const page = await document!.getPage(pageIdx + 1);
          if (cancelledRef.current) {
            page.cleanup();
            return;
          }

          const baseViewport = page.getViewport({ scale: 1 });
          const scale = thumbnailWidth / baseViewport.width;
          const viewport = page.getViewport({ scale });

          const canvas = createCanvas(Math.ceil(viewport.width), Math.ceil(viewport.height));
          const context = canvas.getContext("2d");
          if (!context) {
            page.cleanup();
            continue;
          }

          // eslint-disable-next-line @typescript-eslint/no-explicit-any -- OffscreenCanvas 互換
          const renderTask = page.render({
            canvasContext: context as CanvasRenderingContext2D,
            viewport,
            canvas,
          } as any);
          await renderTask.promise;

          if (cancelledRef.current) {
            page.cleanup();
            return;
          }

          const blobUrl = await canvasToBlobUrl(canvas);
          blobUrlsRef.current.push(blobUrl);

          setThumbnails((prev) => {
            const next = [...prev];
            next[pageIdx] = blobUrl;
            return next;
          });

          page.cleanup();
        } catch {
          // 描画失敗は無視 (null のまま)
        }
      }

      // 次のバッチを setTimeout で遅延スケジュール (UI ブロック回避)
      setTimeout(renderBatch, 0);
    }

    setTimeout(renderBatch, 0);

    return () => {
      cancelledRef.current = true;
      for (const url of blobUrlsRef.current) {
        URL.revokeObjectURL(url);
      }
      blobUrlsRef.current = [];
    };
  }, [document, thumbnailWidth, currentIndex]);

  return { thumbnails, isComplete };
}
