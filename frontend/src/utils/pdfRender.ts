// PDF 描画スケール計算 + オフスクリーン先行描画（先読み）
// - PdfCanvas と先読みフックで同一の scale / cacheKey を共有するために分離
// - cacheKey は "${pageNumber}:${scale*dpr}" 形式。表示中 PdfCanvas と一致させることで
//   先読み済みページへ advance した際にキャッシュヒットして即時描画される

import type { FitMode } from "../stores/viewerStore";
import type { PageViewport, PDFDocumentProxy, PDFPageProxy } from "../lib/pdfjs";
import type { PdfRenderCache } from "../hooks/usePdfRenderCache";

// 最大 scale を 4.0 に制限 (メモリ保護)
export const MAX_SCALE = 4;

// renderPage で算出する scale 関連の派生値
export interface ScaledRender {
  scale: number;
  dpr: number;
  viewport: PageViewport;
  cssWidth: number;
  cssHeight: number;
  cacheKey: string;
}

// fitMode + container 寸法 + dpr から render 用の scale / viewport / cacheKey を計算する純粋関数
export function computePdfRenderScale(params: {
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

// 指定ページをオフスクリーン canvas に描画し ImageBitmap をキャッシュへ格納する（先読み用）
// - 既にキャッシュ済み / createImageBitmap 非対応 / 寸法不正 の場合は何もしない
// - 表示中 PdfCanvas と同一 cacheKey を使うため advance 時に即ヒットする
// - 失敗は黙殺（先読みは best-effort）
export async function renderPdfPageToCache(params: {
  document: PDFDocumentProxy;
  pageNumber: number;
  fitMode: FitMode;
  containerWidth: number;
  containerHeight: number;
  cache: PdfRenderCache;
  isCancelled: () => boolean;
}): Promise<void> {
  if (typeof createImageBitmap === "undefined") {
    return;
  }
  if (params.containerWidth <= 0 || params.containerHeight <= 0) {
    return;
  }
  const page = await params.document.getPage(params.pageNumber);
  try {
    if (params.isCancelled()) {
      return;
    }
    const render = computePdfRenderScale({
      page,
      pageNumber: params.pageNumber,
      fitMode: params.fitMode,
      containerWidth: params.containerWidth,
      containerHeight: params.containerHeight,
    });
    // 既にキャッシュ済みなら再描画しない
    if (params.cache.get(render.cacheKey)) {
      return;
    }
    const canvas = document.createElement("canvas");
    canvas.width = render.viewport.width;
    canvas.height = render.viewport.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }
    await page.render({ canvas, canvasContext: ctx, viewport: render.viewport }).promise;
    if (params.isCancelled()) {
      return;
    }
    const bitmap = await createImageBitmap(canvas);
    params.cache.put(render.cacheKey, bitmap);
  } catch {
    // 先読み失敗は無視
  } finally {
    page.cleanup();
  }
}
