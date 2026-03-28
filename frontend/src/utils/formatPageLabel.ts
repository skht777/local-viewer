// ページラベル文字列を生成する
// - 見開き時: "セット名 3-4 / 12"
// - 単ページ: "セット名 3 / 12"
// - セット名なし: "3 / 12"
export function formatPageLabel(
  setName: string,
  current: number,
  total: number,
  currentEnd?: number,
): string {
  const pageRange =
    currentEnd && currentEnd !== current ? `${current}-${currentEnd}` : `${current}`;
  return setName ? `${setName} ${pageRange} / ${total}` : `${pageRange} / ${total}`;
}
