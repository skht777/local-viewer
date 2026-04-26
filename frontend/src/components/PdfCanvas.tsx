// PDF 1ページを canvas に描画するコンポーネント
// - fitMode に応じた scale 計算 (width/height/original)
// - devicePixelRatio 考慮 (retina 対応)
// - RenderTask の cancel + page.cleanup で安全なライフサイクル管理
// - 最大 scale を 4.0 に制限 (メモリ保護)
// - 描画タイムアウト (15秒) でフリーズ防止
// - enableTextLayer: テキスト選択用の透明テキストレイヤーオーバーレイ

import { useEffect, useRef, useState } from "react";
import type { PDFDocumentProxy, RenderTask } from "../lib/pdfjs";
import { TextLayer } from "../lib/pdfjs";
import type { FitMode } from "../stores/viewerStore";
import type { PdfRenderCache } from "../hooks/usePdfRenderCache";

const MAX_SCALE = 4;
const RENDER_TIMEOUT_MS = 15_000;

interface PdfCanvasProps {
  document: PDFDocumentProxy;
  pageNumber: number;
  fitMode: FitMode;
  containerWidth: number;
  containerHeight: number;
  className?: string;
  renderCache?: PdfRenderCache;
  enableTextLayer?: boolean;
  onRenderComplete?: () => void;
}

// テキストレイヤーコンテナの子要素を安全にクリアする
function clearChildren(container: HTMLElement): void {
  while (container.firstChild) {
    container.firstChild.remove();
  }
}

export function PdfCanvas({
  document,
  pageNumber,
  fitMode,
  containerWidth,
  containerHeight,
  className,
  renderCache,
  enableTextLayer = false,
  onRenderComplete,
}: PdfCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const textLayerRef = useRef<HTMLDivElement>(null);
  const [renderError, setRenderError] = useState(false);

  // ページ変更時に renderError をリセット
  useEffect(() => {
    setRenderError(false);
  }, [pageNumber]);

  useEffect(() => {
    let cancelled = false;
    let renderTask: RenderTask | null = null;
    let timeoutId: ReturnType<typeof setTimeout> | undefined = undefined;

    // 受入: 計画外の新規違反。PDF 描画パイプラインは getPage→viewport→render→
    // textLayer→cleanup を一連で扱い、途中分割は副作用とライフサイクルの相関を
    // わかりにくくする。別タスクで段階分解する。
    // oxlint-disable-next-line max-statements
    async function renderPage() {
      const page = await document.getPage(pageNumber);
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
      const computeScale = (): number => {
        switch (fitMode) {
          case "width":
            return containerWidth / baseViewport.width;
          case "height":
            return containerHeight / baseViewport.height;
          default:
            return 1;
        }
      };
      // 最大 scale 制限
      const scale = Math.min(computeScale(), MAX_SCALE);

      // retina 対応
      const dpr = window.devicePixelRatio || 1;
      const viewport = page.getViewport({ scale: scale * dpr });

      // canvas のピクセルサイズ (retina 解像度)
      canvas.width = viewport.width;
      canvas.height = viewport.height;

      // CSS サイズ (論理ピクセル)
      const cssWidth = viewport.width / dpr;
      const cssHeight = viewport.height / dpr;
      canvas.style.width = `${cssWidth}px`;
      canvas.style.height = `${cssHeight}px`;

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
          // テキストレイヤーも描画 (キャッシュヒット時)
          if (enableTextLayer && textLayerRef.current) {
            await renderTextLayerOverlay(page, textLayerRef.current, scale, cssWidth, cssHeight);
          }
          page.cleanup();
          onRenderComplete?.();
          return;
        }
      }

      renderTask = page.render({ canvas, canvasContext: context, viewport });

      // 描画タイムアウト
      timeoutId = setTimeout(() => {
        renderTask?.cancel();
        if (!cancelled) {
          setRenderError(true);
        }
      }, RENDER_TIMEOUT_MS);

      try {
        await renderTask.promise;
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
        // テキストレイヤー描画
        if (!cancelled && enableTextLayer && textLayerRef.current) {
          await renderTextLayerOverlay(page, textLayerRef.current, scale, cssWidth, cssHeight);
        }
        if (!cancelled) {
          onRenderComplete?.();
        }
      } catch (error) {
        clearTimeout(timeoutId);
        const err = error as { name?: string };
        if (err?.name !== "RenderingCancelledException") {
          // eslint-disable-next-line no-console
          console.error("PDF render error:", error);
        }
      } finally {
        page.cleanup();
      }
    }
    renderPage();

    // cleanup 時に参照が変わっている可能性があるためローカルにコピー
    const textLayerEl = textLayerRef.current;

    return () => {
      cancelled = true;
      clearTimeout(timeoutId);
      renderTask?.cancel();
      // テキストレイヤーをクリア (DOM 安全操作)
      if (textLayerEl) {
        clearChildren(textLayerEl);
      }
    };
  }, [
    document,
    pageNumber,
    fitMode,
    containerWidth,
    containerHeight,
    renderCache,
    enableTextLayer,
    onRenderComplete,
  ]);

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

  return (
    <div className={`relative ${className ?? ""}`}>
      <canvas ref={canvasRef} />
      {enableTextLayer && (
        <div
          ref={textLayerRef}
          className="textLayer"
          // テキスト選択中のクリックがページ送りに伝播しないよう stopPropagation
          onPointerDown={(e) => e.stopPropagation()}
        />
      )}
    </div>
  );
}

// テキストレイヤーを描画する
// - page.getTextContent() + TextLayer で透明テキスト div を重畳
async function renderTextLayerOverlay(
  page: Awaited<ReturnType<PDFDocumentProxy["getPage"]>>,
  container: HTMLDivElement,
  scale: number,
  cssWidth: number,
  cssHeight: number,
): Promise<void> {
  // 既存のテキストレイヤーを安全にクリア
  clearChildren(container);
  container.style.width = `${cssWidth}px`;
  container.style.height = `${cssHeight}px`;

  try {
    const textContent = await page.getTextContent();
    const viewport = page.getViewport({ scale });
    const textLayer = new TextLayer({
      textContentSource: textContent,
      container,
      viewport,
    });
    await textLayer.render();
  } catch {
    // テキストレイヤー描画失敗は無視
  }
}
