// PDF ページ番号サイドバー
// - ThumbnailSidebar の簡易版 (canvas サムネイルは Phase 6.5)
// - ページ番号ボタンのリストでクリックジャンプ
// - アクティブページをハイライト + scrollIntoView

import { useEffect, useRef } from "react";

interface PdfPageSidebarProps {
  pageCount: number;
  currentIndex: number;
  onSelect: (index: number) => void;
  scrollBehavior?: ScrollBehavior;
}

export function PdfPageSidebar({
  pageCount,
  currentIndex,
  onSelect,
  scrollBehavior = "smooth",
}: PdfPageSidebarProps) {
  const activeRef = useRef<HTMLButtonElement>(null);

  // アクティブページを自動スクロール
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", behavior: scrollBehavior });
  }, [currentIndex, scrollBehavior]);

  return (
    <aside className="flex w-16 flex-shrink-0 flex-col gap-1 overflow-y-auto bg-gray-900/80 p-1">
      {Array.from({ length: pageCount }, (_, i) => {
        const isActive = i === currentIndex;
        return (
          <button
            key={i}
            ref={isActive ? activeRef : undefined}
            type="button"
            onClick={() => onSelect(i)}
            className={`rounded px-2 py-1 text-center text-xs ${isActive ? "bg-blue-600 text-white" : "text-gray-400 hover:bg-gray-700 hover:text-white"}`}
          >
            {i + 1}
          </button>
        );
      })}
    </aside>
  );
}
