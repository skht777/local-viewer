// PdfCgViewer / PdfMangaViewer 共通のローディング・エラー画面
// - usePdfDocument の isLoading / error を反映
// - エラー時は閉じるボタンで onClose を呼ぶ

import type { ReactElement } from "react";

interface PdfViewerLoadingProps {
  message?: string;
}

export function PdfViewerLoading({ message = "PDF を読み込み中..." }: PdfViewerLoadingProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black">
      <p className="text-gray-400" data-testid="pdf-loading">
        {message}
      </p>
    </div>
  );
}

interface PdfViewerErrorProps {
  error: Error;
  onClose: () => void;
}

export function PdfViewerError({ error, onClose }: PdfViewerErrorProps) {
  return (
    <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 bg-black">
      <p className="text-red-400" data-testid="pdf-error">
        PDF を開けません: {error.message}
      </p>
      <button
        type="button"
        onClick={onClose}
        className="rounded bg-surface-raised px-4 py-2 text-white hover:bg-surface-overlay"
      >
        閉じる
      </button>
    </div>
  );
}

interface RenderPdfStatusParams {
  isLoading: boolean;
  error: Error | null;
  document: unknown;
  onClose: () => void;
}

interface PdfStatusResult {
  shouldEarlyReturn: boolean;
  element: ReactElement | null;
}

// usePdfDocument の状態 (loading / error / document 不在) に応じた早期 return の指示を返す
// - shouldEarlyReturn=false なら呼び出し側は通常レンダリングへ進む
// - true の場合 element を返す (PdfViewerLoading / PdfViewerError / null)
export function renderPdfStatus({
  isLoading,
  error,
  document,
  onClose,
}: RenderPdfStatusParams): PdfStatusResult {
  if (isLoading) {
    return { shouldEarlyReturn: true, element: <PdfViewerLoading /> };
  }
  if (error) {
    return { shouldEarlyReturn: true, element: <PdfViewerError error={error} onClose={onClose} /> };
  }
  if (!document) {
    return { shouldEarlyReturn: true, element: null };
  }
  return { shouldEarlyReturn: false, element: null };
}
