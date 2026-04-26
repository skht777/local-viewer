// スクロール検出 index を URL に同期
// - debounceMs: number  → 指定 ms の debounce で onChange 呼び出し（MangaViewer は 200ms）
// - debounceMs: null    → 即時呼び出し（PdfMangaViewer の initialPage 比較経路）
//
// 初期マウント時の virtualizer 再計測・画像遅延ロードでスクロール位置が揺らぐと、
// 毎フレーム setSearchParams 連鎖で React の update depth 制限（#185）に到達する
// ことがあるため、画像系は debounce を必ず噛ませる

import { useEffect } from "react";

interface UseUrlIndexSyncParams {
  // スクロール位置から検出した現在 index（0-based）
  currentIndex: number;
  // URL 側の現在 index（0-based）。一致するときは onChange を呼ばない
  externalIndex: number;
  // 反映 callback。debounceMs > 0 なら setTimeout 経由
  onChange: (index: number) => void;
  // null = 即時、number = debounce ms
  debounceMs: number | null;
}

export function useUrlIndexSync({
  currentIndex,
  externalIndex,
  onChange,
  debounceMs,
}: UseUrlIndexSyncParams): void {
  useEffect(() => {
    if (currentIndex === externalIndex) {
      return;
    }
    if (debounceMs === null) {
      onChange(currentIndex);
      return;
    }
    const timer = setTimeout(() => {
      onChange(currentIndex);
    }, debounceMs);
    return () => clearTimeout(timer);
  }, [currentIndex, externalIndex, onChange, debounceMs]);
}
