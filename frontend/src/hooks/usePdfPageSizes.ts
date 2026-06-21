// PDF の estimateSize 用ページサイズを軽量に取得
// - 先頭ページのみ getPage し、そのサイズを全ページの推定値とする（均一PDF前提）
// - 実際に表示されるページは PdfMangaViewer の virtualizer.measureElement が再計測するため、
//   可変サイズPDFでもスクロール後はスクロール高さが補正される
// - disableAutoFetch 環境で全ページ getPage による range リクエスト多発を避け、初期化を高速化

import { useEffect, useRef, useState } from "react";
import type { PDFDocumentProxy } from "../lib/pdfjs";

export interface PageSize {
  width: number;
  height: number;
}

interface UsePdfPageSizesReturn {
  pageSizes: PageSize[];
  isReady: boolean;
}

export function usePdfPageSizes(document: PDFDocumentProxy | null): UsePdfPageSizesReturn {
  const [pageSizes, setPageSizes] = useState<PageSize[]>([]);
  const [isReady, setIsReady] = useState(false);
  const cancelledRef = useRef(false);

  useEffect(() => {
    if (!document) {
      setPageSizes([]);
      setIsReady(false);
      return;
    }

    cancelledRef.current = false;
    const { numPages } = document;
    const pdfDocument = document;

    // 先頭ページのサイズをサンプリングし、全ページの推定値とする
    async function sampleFirstPage() {
      const page = await pdfDocument.getPage(1);
      const vp = page.getViewport({ scale: 1 });
      page.cleanup();
      if (cancelledRef.current) {
        return;
      }
      const size: PageSize = { width: vp.width, height: vp.height };
      setPageSizes(Array.from({ length: numPages }, () => size));
      setIsReady(true);
    }

    sampleFirstPage();

    return () => {
      cancelledRef.current = true;
    };
  }, [document]);

  return { pageSizes, isReady };
}
