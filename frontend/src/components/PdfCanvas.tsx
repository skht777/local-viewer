// PDF 1ページを canvas に描画するコンポーネント
// - fitMode に応じた scale 計算 (width/height/original)
// - devicePixelRatio 考慮 (retina 対応)
// - RenderTask の cancel + page.cleanup で安全なライフサイクル管理
// - 最大 scale を 4.0 に制限 (メモリ保護)
// - 描画タイムアウト (15秒) でフリーズ防止

import { useEffect, useRef, useState } from "react";
import type { PDFDocumentProxy, RenderTask } from "../lib/pdfjs";
import type { FitMode } from "../stores/viewerStore";
import type { PdfRenderCache } from "../hooks/usePdfRenderCache";

const MAX_SCALE = 4.0;
const RENDER_TIMEOUT_MS = 15_000;

interface PdfCanvasProps {
  document: PDFDocumentProxy;
  pageNumber: number;
  fitMode: FitMode;
  containerWidth: number;
  containerHeight: number;
  className?: string;
  renderCache?: PdfRenderCache;
  onRenderComplete?: () => void;
}

export function PdfCanvas({
  document,
  pageNumber,
  fitMode,
  containerWidth,
  containerHeight,
  className,
  renderCache,
  onRenderComplete,
}: PdfCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [renderError, setRenderError] = useState(false);

  // ページ変更時に renderError をリセット
  useEffect(() => {
    setRenderError(false);
  }, [pageNumber]);

  useEffect(() => {
    let cancelled = false;
    let renderTask: RenderTask | null = null;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;

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

      // キャッシュキー: "pageNumber:effectiveScale"
      const cacheKey = `${pageNumber}:${scale * dpr}`;

      // キャッシュヒット: ImageBitmap から即座に描画
      if (renderCache) {
        const cached = renderCache.get(cacheKey);
        if (cached) {
          context.drawImage(cached, 0, 0);
          page.cleanup();
          onRenderComplete?.();
          return;
        }
      }

      renderTask = page.render({ canvas, canvasContext: context, viewport });

      // 描画タイムアウト
      timeoutId = setTimeout(() => {
        renderTask?.cancel();
        if (!cancelled) setRenderError(true);
      }, RENDER_TIMEOUT_MS);

      renderTask.promise
        .then(async () => {
          clearTimeout(timeoutId);
          // キャッシュに格納
          if (!cancelled && renderCache && typeof createImageBitmap !== "undefined") {
            try {
              const bitmap = await createImageBitmap(canvas);
              renderCache.put(cacheKey, bitmap);
            } catch {
              // createImageBitmap 失敗は無視
            }
          }
          if (!cancelled) onRenderComplete?.();
        })
        .catch((err: { name?: string }) => {
          clearTimeout(timeoutId);
          // cancel() による中断は正常動作 (タイムアウトによるキャンセルを除く)
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
      clearTimeout(timeoutId);
      renderTask?.cancel();
    };
  }, [document, pageNumber, fitMode, containerWidth, containerHeight, onRenderComplete]);

  // タイムアウトエラー表示
  if (renderError) {
    return (
      <div
        className={`flex items-center justify-center ${className ?? ""}`}
        data-testid="pdf-render-error"
      >
        <p className="text-sm text-red-400">描画がタイムアウトしました</p>
      </div>
    );
  }

  return <canvas ref={canvasRef} className={className} />;
}
