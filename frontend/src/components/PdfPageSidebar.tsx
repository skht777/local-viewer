// PDF ページサムネイルサイドバー
// - usePdfThumbnails で canvas サムネイルを順次生成
// - サムネイル取得済み → <img> 表示、未取得 → ページ番号フォールバック
// - アクティブページをハイライト + scrollIntoView

import { useEffect, useRef } from "react";
import type { PDFDocumentProxy } from "../lib/pdfjs";
import { usePdfThumbnails } from "../hooks/usePdfThumbnails";

interface PdfPageSidebarProps {
  document: PDFDocumentProxy | null;
  pageCount: number;
  currentIndex: number;
  onSelect: (index: number) => void;
  scrollBehavior?: ScrollBehavior;
}

export function PdfPageSidebar({
  document,
  pageCount,
  currentIndex,
  onSelect,
  scrollBehavior = "smooth",
}: PdfPageSidebarProps) {
  const activeRef = useRef<HTMLButtonElement>(null);
  const { thumbnails } = usePdfThumbnails(document, currentIndex);

  // アクティブページを自動スクロール
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", behavior: scrollBehavior });
  }, [currentIndex, scrollBehavior]);

  return (
    <aside className="flex w-24 flex-shrink-0 flex-col gap-1 overflow-y-auto bg-gray-900/80 p-1">
      {Array.from({ length: pageCount }, (_, i) => {
        const isActive = i === currentIndex;
        const thumbUrl = thumbnails[i];
        return (
          <button
            key={i}
            ref={isActive ? activeRef : undefined}
            type="button"
            onClick={() => onSelect(i)}
            className={`overflow-hidden rounded ${isActive ? "ring-2 ring-blue-500" : "opacity-60 hover:opacity-100"}`}
          >
            {thumbUrl ? (
              <img
                src={thumbUrl}
                alt={`Page ${i + 1}`}
                className="h-auto w-full"
                loading="lazy"
                decoding="async"
              />
            ) : (
              <div className="flex h-12 items-center justify-center text-xs text-gray-400">
                {i + 1}
              </div>
            )}
          </button>
        );
      })}
    </aside>
  );
}
