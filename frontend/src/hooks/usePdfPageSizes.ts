// PDF 全ページの viewport サイズを事前取得
// - PdfMangaViewer の estimateSize に正確な高さを提供
// - バッチ処理で getPage burst を抑制 (大規模 PDF でのメモリスパイク防止)
// - 全バッチ完了後に一括で状態更新

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

// バッチあたりの同時 getPage 呼び出し数
const BATCH_SIZE = 10;

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

    // バッチ処理で getPage burst を抑制
    // - BATCH_SIZE ページずつ getPage + getViewport
    // - バッチ間で setTimeout(_, 0) により UI スレッドに譲る
    async function loadInBatches() {
      const sizes: PageSize[] = [];
      for (let i = 0; i < numPages; i += BATCH_SIZE) {
        const end = Math.min(i + BATCH_SIZE, numPages);
        const batch = await Promise.all(
          Array.from({ length: end - i }, (_, j) =>
            document!.getPage(i + j + 1).then((page) => {
              const vp = page.getViewport({ scale: 1 });
              page.cleanup();
              return { width: vp.width, height: vp.height };
            }),
          ),
        );
        if (cancelledRef.current) return;
        sizes.push(...batch);
        // UI スレッドに譲る (最終バッチ以外)
        if (end < numPages) {
          await new Promise<void>((r) => setTimeout(r, 0));
        }
      }
      if (!cancelledRef.current) {
        setPageSizes(sizes);
        setIsReady(true);
      }
    }

    loadInBatches();

    return () => {
      cancelledRef.current = true;
    };
  }, [document]);

  return { pageSizes, isReady };
}
