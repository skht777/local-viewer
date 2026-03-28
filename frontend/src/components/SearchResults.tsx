// 検索結果ドロップダウン
// - kind アイコン + ファイル名 + 相対パス表示
// - キーボード選択 (activeIndex) のハイライト
// - クリックで onSelect 呼び出し

import type { SearchResult } from "../types/api";

const KIND_ICONS: Record<string, string> = {
  directory: "\u{1F4C1}",
  image: "\u{1F5BC}",
  video: "\u{1F3AC}",
  pdf: "\u{1F4C4}",
  archive: "\u{1F4E6}",
  other: "\u{1F4C4}",
};

interface SearchResultsProps {
  results: SearchResult[];
  hasMore: boolean;
  isLoading: boolean;
  isIndexing: boolean;
  activeIndex: number;
  onSelect: (result: SearchResult) => void;
}

export function SearchResults({
  results,
  hasMore,
  isLoading,
  isIndexing,
  activeIndex,
  onSelect,
}: SearchResultsProps) {
  if (isIndexing) {
    return (
      <div className="absolute z-50 mt-1 w-full rounded-lg bg-gray-800 p-4 text-gray-400 shadow-lg">
        インデックス構築中...
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="absolute z-50 mt-1 w-full rounded-lg bg-gray-800 p-4 text-gray-400 shadow-lg">
        検索中...
      </div>
    );
  }

  if (results.length === 0) {
    return (
      <div className="absolute z-50 mt-1 w-full rounded-lg bg-gray-800 p-4 text-gray-400 shadow-lg">
        結果が見つかりません
      </div>
    );
  }

  return (
    <div
      className="absolute z-50 mt-1 max-h-96 w-full overflow-y-auto rounded-lg bg-gray-800 shadow-lg"
      data-testid="search-results"
    >
      <ul>
        {results.map((result, i) => (
          <li
            key={result.node_id}
            className={`cursor-pointer px-4 py-2 ${i === activeIndex ? "bg-blue-600/20 hover:bg-blue-600/30" : "hover:bg-gray-700"}`}
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
    </div>
  );
}
