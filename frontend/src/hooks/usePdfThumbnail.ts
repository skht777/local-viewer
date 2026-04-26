// PDF の先頭ページを pdfjs-dist でレンダリングし、blob URL として返すフック
// - getDocument() で PDF をロード → page.render() → canvas.toBlob() → URL.createObjectURL()
// - unmount 時に URL.revokeObjectURL() でクリーンアップ
// - enabled=false の場合はロードしない

import { useEffect, useRef, useState } from "react";
import type { MutableRefObject } from "react";
import type { PDFDocumentProxy, PDFPageProxy } from "../lib/pdfjs";
import { getDocument } from "../lib/pdfjs";

const THUMB_WIDTH = 300;

interface PdfThumbnailResult {
  url: string | null;
  isLoading: boolean;
  hasError: boolean;
}

// PDF の先頭ページを取得する。pdfDoc は呼び出し側が destroy する責務を持つ
async function loadPdfThumbnailPage(
  nodeId: string,
): Promise<{ pdfDoc: PDFDocumentProxy; page: PDFPageProxy }> {
  const pdfDoc = await getDocument({ url: `/api/file/${nodeId}` }).promise;
  const page = await pdfDoc.getPage(1);
  return { pdfDoc, page };
}

// 指定幅に収まるスケールでオフスクリーン canvas にレンダリングし JPEG blob を返す
async function renderThumbnailToBlob(page: PDFPageProxy, maxWidth: number): Promise<Blob | null> {
  const unscaledViewport = page.getViewport({ scale: 1 });
  const scale = maxWidth / unscaledViewport.width;
  const viewport = page.getViewport({ scale });
  const canvas = document.createElement("canvas");
  canvas.width = viewport.width;
  canvas.height = viewport.height;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    throw new Error("Canvas 2D context not available");
  }
  await page.render({ canvasContext: ctx, viewport, canvas }).promise;
  return await new Promise<Blob | null>((resolve) => {
    canvas.toBlob(resolve, "image/jpeg", 0.8);
  });
}

// blob → URL を作成し、ref と state を更新する
function publishBlobUrl(
  blob: Blob,
  urlRef: MutableRefObject<string | null>,
  setUrl: (url: string) => void,
): void {
  const blobUrl = URL.createObjectURL(blob);
  urlRef.current = blobUrl;
  setUrl(blobUrl);
}

export function usePdfThumbnail(nodeId: string, enabled: boolean): PdfThumbnailResult {
  const [url, setUrl] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [hasError, setHasError] = useState(false);
  const urlRef = useRef<string | null>(null);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    let cancelled = false;
    let pdfDoc: PDFDocumentProxy | null = null;

    const generate = async () => {
      setIsLoading(true);
      setHasError(false);
      try {
        const loaded = await loadPdfThumbnailPage(nodeId);
        if (cancelled) {
          return;
        }
        ({ pdfDoc } = loaded);
        const blob = await renderThumbnailToBlob(loaded.page, THUMB_WIDTH);
        if (cancelled || !blob) {
          return;
        }
        publishBlobUrl(blob, urlRef, setUrl);
      } catch {
        if (!cancelled) {
          setHasError(true);
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
        pdfDoc?.destroy();
      }
    };

    generate();

    return () => {
      cancelled = true;
      // 古い blob URL をクリーンアップ
      if (urlRef.current) {
        URL.revokeObjectURL(urlRef.current);
        urlRef.current = null;
      }
    };
  }, [nodeId, enabled]);

  return { url, isLoading, hasError };
}
