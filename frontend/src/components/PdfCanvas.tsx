// PDF 1ページを canvas に描画するコンポーネント
// - fitMode に応じた scale 計算 (width/height/original)
// - devicePixelRatio 考慮 (retina 対応)
// - RenderTask の cancel + page.cleanup で安全なライフサイクル管理
// - 最大 scale を 4.0 に制限 (メモリ保護)

import { useEffect, useRef } from "react";
import type { PDFDocumentProxy, RenderTask } from "../lib/pdfjs";
import type { FitMode } from "../stores/viewerStore";

const MAX_SCALE = 4.0;

interface PdfCanvasProps {
  document: PDFDocumentProxy;
  pageNumber: number;
  fitMode: FitMode;
  containerWidth: number;
  containerHeight: number;
  className?: string;
  onRenderComplete?: () => void;
}

export function PdfCanvas({
  document,
  pageNumber,
  fitMode,
  containerWidth,
  containerHeight,
  className,
  onRenderComplete,
}: PdfCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    let cancelled = false;
    let renderTask: RenderTask | null = null;

    document.getPage(pageNumber).then((page) => {
      if (cancelled) {
        page.cleanup();
        return;
      }

      const canvas = canvasRef.current;
      if (!canvas) {
        page.cleanup();
        return;
      }

      // オリジナルサイズ取得
      const baseViewport = page.getViewport({ scale: 1 });

      // fitMode に応じた scale 計算
      let scale: number;
      switch (fitMode) {
        case "width":
          scale = containerWidth / baseViewport.width;
          break;
        case "height":
          scale = containerHeight / baseViewport.height;
          break;
        case "original":
        default:
          scale = 1.0;
          break;
      }

      // 最大 scale 制限
      scale = Math.min(scale, MAX_SCALE);

      // retina 対応
      const dpr = window.devicePixelRatio || 1;
      const viewport = page.getViewport({ scale: scale * dpr });

      // canvas のピクセルサイズ (retina 解像度)
      canvas.width = viewport.width;
      canvas.height = viewport.height;

      // CSS サイズ (論理ピクセル)
      canvas.style.width = `${viewport.width / dpr}px`;
      canvas.style.height = `${viewport.height / dpr}px`;

      const context = canvas.getContext("2d");
      if (!context) {
        page.cleanup();
        return;
      }

      renderTask = page.render({ canvasContext: context, viewport });
      renderTask.promise
        .then(() => {
          if (!cancelled) onRenderComplete?.();
        })
        .catch((err: { name?: string }) => {
          // cancel() による中断は正常動作
          if (err?.name !== "RenderingCancelledException") {
            // eslint-disable-next-line no-console
            console.error("PDF render error:", err);
          }
        })
        .finally(() => {
          page.cleanup();
        });
    });

    return () => {
      cancelled = true;
      renderTask?.cancel();
    };
  }, [document, pageNumber, fitMode, containerWidth, containerHeight, onRenderComplete]);

  return <canvas ref={canvasRef} className={className} />;
}
