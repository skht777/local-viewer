// PDF 全ページの viewport サイズを事前取得
// - PdfMangaViewer の estimateSize に正確な高さを提供
// - ドキュメント読み込み完了後に全ページ分の getPage + getViewport を呼び出し

import { useEffect, useState } from "react";
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

  useEffect(() => {
    if (!document) {
      setPageSizes([]);
      setIsReady(false);
      return;
    }

    let cancelled = false;
    const { numPages } = document;

    // 全ページの viewport サイズを取得
    const promises = Array.from({ length: numPages }, (_, i) =>
      document.getPage(i + 1).then((page) => {
        const viewport = page.getViewport({ scale: 1 });
        page.cleanup();
        return { width: viewport.width, height: viewport.height };
      }),
    );

    Promise.all(promises).then((sizes) => {
      if (cancelled) return;
      setPageSizes(sizes);
      setIsReady(true);
    });

    return () => {
      cancelled = true;
    };
  }, [document]);

  return { pageSizes, isReady };
}
