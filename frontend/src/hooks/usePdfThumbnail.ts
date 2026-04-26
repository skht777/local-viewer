// PDF の先頭ページを pdfjs-dist でレンダリングし、blob URL として返すフック
// - getDocument() で PDF をロード → page.render() → canvas.toBlob() → URL.createObjectURL()
// - unmount 時に URL.revokeObjectURL() でクリーンアップ
// - enabled=false の場合はロードしない

import { useEffect, useRef, useState } from "react";
import type { PDFDocumentProxy } from "../lib/pdfjs";
import { getDocument } from "../lib/pdfjs";

const THUMB_WIDTH = 300;

interface PdfThumbnailResult {
  url: string | null;
  isLoading: boolean;
  hasError: boolean;
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
        const loadingTask = getDocument({ url: `/api/file/${nodeId}` });
        pdfDoc = await loadingTask.promise;
        if (cancelled) {
          return;
        }

        const page = await pdfDoc.getPage(1);
        if (cancelled) {
          return;
        }

        // 300px 幅に収まるスケールを計算
        const unscaledViewport = page.getViewport({ scale: 1 });
        const scale = THUMB_WIDTH / unscaledViewport.width;
        const viewport = page.getViewport({ scale });

        // オフスクリーン Canvas にレンダリング
        const canvas = document.createElement("canvas");
        canvas.width = viewport.width;
        canvas.height = viewport.height;
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          throw new Error("Canvas 2D context not available");
        }

        await page.render({ canvasContext: ctx, viewport, canvas }).promise;
        if (cancelled) {
          return;
        }

        // Canvas → blob → URL
        const blob = await new Promise<Blob | null>((resolve) =>
          canvas.toBlob(resolve, "image/jpeg", 0.8),
        );
        if (cancelled || !blob) {
          return;
        }

        const blobUrl = URL.createObjectURL(blob);
        urlRef.current = blobUrl;
        setUrl(blobUrl);
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
