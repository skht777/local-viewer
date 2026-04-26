// SearchBar のドロップダウン開閉と activeIndex 制御
// - debouncedQuery が 2 文字以上で開く / それ未満で閉じる
// - 開閉のたびに activeIndex を -1 にリセット
// - containerRef 外クリックで閉じる

import { useEffect, useState } from "react";

interface UseSearchDropdownParams {
  debouncedQuery: string;
  containerRef: React.RefObject<HTMLElement | null>;
}

interface UseSearchDropdownResult {
  isOpen: boolean;
  setIsOpen: (open: boolean) => void;
  activeIndex: number;
  setActiveIndex: React.Dispatch<React.SetStateAction<number>>;
}

const MIN_QUERY_LENGTH = 2;

export function useSearchDropdown({
  debouncedQuery,
  containerRef,
}: UseSearchDropdownParams): UseSearchDropdownResult {
  const [isOpen, setIsOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);

  // 結果が更新されたらドロップダウンを開く
  useEffect(() => {
    if (debouncedQuery.length >= MIN_QUERY_LENGTH) {
      setIsOpen(true);
      setActiveIndex(-1);
    } else {
      setIsOpen(false);
    }
  }, [debouncedQuery]);

  // 外側クリックで閉じる
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [containerRef]);

  return { isOpen, setIsOpen, activeIndex, setActiveIndex };
}
