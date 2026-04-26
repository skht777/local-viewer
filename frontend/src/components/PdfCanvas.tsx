// PDF 1ページを canvas に描画するコンポーネント
// - fitMode に応じた scale 計算 (width/height/original)
// - devicePixelRatio 考慮 (retina 対応)
// - RenderTask の cancel + page.cleanup で安全なライフサイクル管理
// - 最大 scale を 4.0 に制限 (メモリ保護)
// - 描画タイムアウト (15秒) でフリーズ防止
// - enableTextLayer: テキスト選択用の透明テキストレイヤーオーバーレイ

import { useEffect, useRef, useState } from "react";
import type { PageViewport, PDFDocumentProxy, PDFPageProxy } from "../lib/pdfjs";
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

// renderPage で算出する scale 関連の派生値
interface ScaledRender {
  scale: number;
  dpr: number;
  viewport: PageViewport;
  cssWidth: number;
  cssHeight: number;
  cacheKey: string;
}

// fitMode + container 寸法 + dpr から render 用の scale / viewport / cacheKey を計算する純粋関数
function computePdfRenderScale(params: {
  page: PDFPageProxy;
  pageNumber: number;
  fitMode: FitMode;
  containerWidth: number;
  containerHeight: number;
}): ScaledRender {
  const baseViewport = params.page.getViewport({ scale: 1 });
  const rawScale =
    params.fitMode === "width"
      ? params.containerWidth / baseViewport.width
      : params.fitMode === "height"
        ? params.containerHeight / baseViewport.height
        : 1;
  const scale = Math.min(rawScale, MAX_SCALE);
  const dpr = window.devicePixelRatio || 1;
  const viewport = params.page.getViewport({ scale: scale * dpr });
  return {
    scale,
    dpr,
    viewport,
    cssWidth: viewport.width / dpr,
    cssHeight: viewport.height / dpr,
    cacheKey: `${params.pageNumber}:${scale * dpr}`,
  };
}

// canvas のピクセルサイズと CSS 寸法を一括設定する
function setupCanvasDimensions(canvas: HTMLCanvasElement, render: ScaledRender): void {
  canvas.width = render.viewport.width;
  canvas.height = render.viewport.height;
  canvas.style.width = `${render.cssWidth}px`;
  canvas.style.height = `${render.cssHeight}px`;
}

// PDF.js の RenderTask を開始し promise / cancel / clearTimer を返す
// - cancel は AbortSignal ではなく PDF.js 仕様の RenderTask.cancel() を使う
// - page.cleanup() は呼び出し側 (renderPage 本体の finally / 早期 return) に残す
interface RenderTaskHandle {
  promise: Promise<void>;
  cancel: () => void;
  clearTimer: () => void;
}

function startPdfRenderTask(params: {
  page: PDFPageProxy;
  canvas: HTMLCanvasElement;
  context: CanvasRenderingContext2D;
  viewport: PageViewport;
  timeoutMs: number;
  onTimeout: () => void;
}): RenderTaskHandle {
  const task = params.page.render({
    canvas: params.canvas,
    canvasContext: params.context,
    viewport: params.viewport,
  });
  const timeoutId = setTimeout(() => {
    task.cancel();
    params.onTimeout();
  }, params.timeoutMs);
  return {
    promise: task.promise,
    cancel: () => task.cancel(),
    clearTimer: () => clearTimeout(timeoutId),
  };
}

// キャッシュヒット時: 保存済み bitmap を直描画 + textLayer を必要なら再構築する
// - 内部で page.cleanup() は呼ばない (textLayer 完了前に getTextContent 不能になる)
async function paintCachedBitmap(params: {
  ctx: CanvasRenderingContext2D;
  bitmap: ImageBitmap;
  page: PDFPageProxy;
  textLayerEl: HTMLDivElement | null;
  enableTextLayer: boolean;
  render: ScaledRender;
}): Promise<void> {
  params.ctx.drawImage(params.bitmap, 0, 0);
  if (params.enableTextLayer && params.textLayerEl) {
    await renderTextLayerOverlay(
      params.page,
      params.textLayerEl,
      params.render.scale,
      params.render.cssWidth,
      params.render.cssHeight,
    );
  }
}

// 描画完了後に canvas を ImageBitmap 化してキャッシュへ格納する
async function cacheRenderedBitmap(
  cache: PdfRenderCache,
  canvas: HTMLCanvasElement,
  cacheKey: string,
): Promise<void> {
  if (typeof createImageBitmap === "undefined") {
    return;
  }
  try {
    const bitmap = await createImageBitmap(canvas);
    cache.put(cacheKey, bitmap);
  } catch {
    // createImageBitmap 失敗は無視
  }
}

// RenderTask 完了後の後処理: キャッシュ格納 → textLayer 描画 → 完了通知
async function finishPdfRender(params: {
  page: PDFPageProxy;
  canvas: HTMLCanvasElement;
  render: ScaledRender;
  renderCache: PdfRenderCache | undefined;
  textLayerEl: HTMLDivElement | null;
  enableTextLayer: boolean;
  cancelled: () => boolean;
  onRenderComplete?: () => void;
}): Promise<void> {
  if (!params.cancelled() && params.renderCache) {
    await cacheRenderedBitmap(params.renderCache, params.canvas, params.render.cacheKey);
  }
  if (!params.cancelled() && params.enableTextLayer && params.textLayerEl) {
    await renderTextLayerOverlay(
      params.page,
      params.textLayerEl,
      params.render.scale,
      params.render.cssWidth,
      params.render.cssHeight,
    );
  }
  if (!params.cancelled()) {
    params.onRenderComplete?.();
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
    let handle: RenderTaskHandle | null = null;

    async function renderPage() {
      const page = await document.getPage(pageNumber);
      const canvas = canvasRef.current;
      if (cancelled || !canvas) {
        page.cleanup();
        return;
      }
      const render = computePdfRenderScale({
        page,
        pageNumber,
        fitMode,
        containerWidth,
        containerHeight,
      });
      setupCanvasDimensions(canvas, render);
      const context = canvas.getContext("2d");
      if (!context) {
        page.cleanup();
        return;
      }

      // キャッシュヒット: ImageBitmap から即座に描画 (textLayer 完了後に cleanup)
      const cached = renderCache?.get(render.cacheKey);
      if (cached) {
        try {
          await paintCachedBitmap({
            ctx: context,
            bitmap: cached,
            page,
            textLayerEl: textLayerRef.current,
            enableTextLayer,
            render,
          });
          if (!cancelled) {
            onRenderComplete?.();
          }
        } finally {
          page.cleanup();
        }
        return;
      }

      // キャッシュミス: RenderTask を起動して描画 → キャッシュ → textLayer
      handle = startPdfRenderTask({
        page,
        canvas,
        context,
        viewport: render.viewport,
        timeoutMs: RENDER_TIMEOUT_MS,
        onTimeout: () => {
          if (!cancelled) {
            setRenderError(true);
          }
        },
      });
      try {
        await handle.promise;
        handle.clearTimer();
        await finishPdfRender({
          page,
          canvas,
          render,
          renderCache,
          textLayerEl: textLayerRef.current,
          enableTextLayer,
          cancelled: () => cancelled,
          onRenderComplete,
        });
      } catch (error) {
        handle.clearTimer();
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
      handle?.clearTimer();
      handle?.cancel();
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
