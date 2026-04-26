// PDF ビューワー (CG/Manga) の現在ページ state とページ変更ハンドラ
// - 内部は 0-based index、URL/UI は 1-based ページ番号
// - onPageChange が呼ばれると state 更新 + 呼び出し側へ通知 (URL 同期等)

import { useCallback, useState } from "react";

interface UsePdfPageStateResult {
  currentPage: number;
  handlePageChange: (index: number) => void;
  setCurrentPage: (index: number) => void;
}

export function usePdfPageState(
  initialPage: number,
  onPageChange: (page: number) => void,
): UsePdfPageStateResult {
  const [currentPage, setCurrentPage] = useState(initialPage - 1);

  const handlePageChange = useCallback(
    (index: number) => {
      setCurrentPage(index);
      onPageChange(index + 1);
    },
    [onPageChange],
  );

  return { currentPage, handlePageChange, setCurrentPage };
}
