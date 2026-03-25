// PDF ドキュメントの読み込み・管理・メモリ解放
// - getDocument() の loading task を保持し、cleanup で destroy
// - cancelled フラグで race condition を防止
// - URL 変更時は旧 document + loading task を破棄して再読み込み

import { useEffect, useRef, useState } from "react";
import { getDocument } from "../lib/pdfjs";
import type { PDFDocumentProxy } from "../lib/pdfjs";

interface UsePdfDocumentReturn {
  document: PDFDocumentProxy | null;
  pageCount: number;
  isLoading: boolean;
  error: Error | null;
}

export function usePdfDocument(fileUrl: string): UsePdfDocumentReturn {
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  // 前回の document を ref で保持（cleanup 時に destroy するため）
  const documentRef = useRef<PDFDocumentProxy | null>(null);

  useEffect(() => {
    let cancelled = false;

    // 前回の document を破棄
    if (documentRef.current) {
      documentRef.current.destroy();
      documentRef.current = null;
    }

    setDocument(null);
    setIsLoading(true);
    setError(null);

    const loadingTask = getDocument(fileUrl);

    loadingTask.promise.then(
      (pdf) => {
        if (cancelled) {
          // URL 変更/unmount 後に解決 → 破棄
          pdf.destroy();
          return;
        }
        documentRef.current = pdf;
        setDocument(pdf);
        setIsLoading(false);
      },
      (err: unknown) => {
        if (cancelled) return;
        setError(err instanceof Error ? err : new Error(String(err)));
        setIsLoading(false);
      },
    );

    return () => {
      cancelled = true;
      loadingTask.destroy();
      // 既に解決済みの document は ref 経由で次の effect か unmount で破棄
    };
  }, [fileUrl]);

  // unmount 時に残った document を破棄
  useEffect(() => {
    return () => {
      documentRef.current?.destroy();
    };
  }, []);

  return {
    document,
    pageCount: document?.numPages ?? 0,
    isLoading,
    error,
  };
}
