// 検索結果ドロップダウン
// - kind アイコン + ファイル名 + 相対パス表示
// - キーボード選択 (activeIndex) のハイライト
// - クリックで onSelect 呼び出し

import type { SearchResult } from "../types/api";

const KIND_ICONS: Record<string, string> = {
  archive: "\u{1F4E6}",
  directory: "\u{1F4C1}",
  image: "\u{1F5BC}",
  other: "\u{1F4C4}",
  pdf: "\u{1F4C4}",
  video: "\u{1F3AC}",
};

interface SearchResultsProps {
  results: SearchResult[];
  hasMore: boolean;
  isLoading: boolean;
  isIndexing: boolean;
  isError?: boolean;
  activeIndex: number;
  onSelect: (result: SearchResult) => void;
  onRetry?: () => void;
  /** 「すべて表示」ボタン押下時のハンドラ（/search ページに遷移） */
  onShowAll?: () => void;
}

export function SearchResults({
  results,
  hasMore,
  isLoading,
  isIndexing,
  isError,
  activeIndex,
  onSelect,
  onRetry,
  onShowAll,
}: SearchResultsProps) {
  if (isIndexing) {
    return (
      <div className="absolute z-50 mt-1 w-full rounded-lg bg-surface-raised p-4 text-gray-400 shadow-lg">
        インデックス構築中...
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="absolute z-50 mt-1 w-full rounded-lg bg-surface-raised p-4 text-gray-400 shadow-lg">
        検索中...
      </div>
    );
  }

  if (isError) {
    return (
      <div className="absolute z-50 mt-1 w-full rounded-lg bg-surface-raised p-4 shadow-lg">
        <p className="text-red-400">検索に失敗しました</p>
        {onRetry && (
          <button
            type="button"
            onClick={onRetry}
            className="mt-2 rounded bg-blue-600 px-3 py-1 text-xs text-white hover:bg-blue-500"
          >
            再試行
          </button>
        )}
      </div>
    );
  }

  if (results.length === 0) {
    return (
      <div className="absolute z-50 mt-1 w-full rounded-lg bg-surface-raised p-4 text-gray-400 shadow-lg">
        結果が見つかりません
      </div>
    );
  }

  return (
    <div
      className="absolute z-50 mt-1 max-h-96 w-full overflow-y-auto rounded-lg bg-surface-raised shadow-lg"
      data-testid="search-results"
    >
      <ul>
        {results.map((result, i) => (
          <li
            key={result.node_id}
            className={`cursor-pointer px-4 py-2 ${i === activeIndex ? "bg-blue-600/20 hover:bg-blue-600/30" : "hover:bg-surface-overlay"}`}
            onClick={() => onSelect(result)}
            data-testid={`search-result-${i}`}
            aria-selected={i === activeIndex ? "true" : undefined}
          >
            <div className="flex items-center gap-2">
              <span>{KIND_ICONS[result.kind] ?? KIND_ICONS.other}</span>
              <span className="truncate font-medium text-white">{result.name}</span>
            </div>
            <div className="ml-6 truncate text-xs text-gray-500">{result.relative_path}</div>
          </li>
        ))}
      </ul>
      {hasMore && (
        <div className="px-4 py-2 text-center text-xs text-gray-500">さらに結果があります...</div>
      )}
      {onShowAll && (
        <button
          type="button"
          onClick={onShowAll}
          data-testid="search-show-all"
          className="block w-full border-t border-surface-overlay px-4 py-2 text-center text-sm text-blue-400 hover:bg-surface-overlay hover:text-blue-300"
        >
          すべて表示
        </button>
      )}
    </div>
  );
}
