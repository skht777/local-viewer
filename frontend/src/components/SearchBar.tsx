// キーワード検索バー
// - デバウンス入力 + ライブ結果ドロップダウン
// - kind フィルタボタン
// - ↑↓ キーで結果選択、Enter で遷移、Escape で閉じる
// - 検索バーにフォーカス中はビューワーのキーボードショートカット無効化 (既存動作)

import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useSearch } from "../hooks/useSearch";
import type { SearchResult } from "../types/api";
import { SearchResults } from "./SearchResults";

const KIND_FILTERS = [
  { label: "All", value: null },
  { label: "\u{1F4C1}", value: "directory" },
  { label: "\u{1F3AC}", value: "video" },
  { label: "\u{1F4C4}", value: "pdf" },
  { label: "\u{1F4E6}", value: "archive" },
] as const;

interface SearchBarProps {
  scope?: string;
}

export function SearchBar({ scope }: SearchBarProps) {
  const navigate = useNavigate();
  // スコープ切替: scope プロップがある場合のみ有効
  const [isScopeActive, setIsScopeActive] = useState(true);
  const effectiveScope = scope && isScopeActive ? scope : undefined;

  const {
    query,
    setQuery,
    debouncedQuery,
    kind,
    setKind,
    results,
    hasMore,
    isLoading,
    isError,
    isIndexing,
    refetch,
  } = useSearch(effectiveScope);

  const [isOpen, setIsOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // ドロップダウンの開閉
  const shouldShowDropdown = isOpen && debouncedQuery.length >= 2;

  // 結果が更新されたらドロップダウンを開く
  useEffect(() => {
    if (debouncedQuery.length >= 2) {
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
  }, []);

  // 検索結果の選択 → ナビゲーション
  const handleSelect = useCallback(
    (result: SearchResult) => {
      setIsOpen(false);
      setQuery("");

      if (result.kind === "directory" || result.kind === "archive") {
        // ディレクトリ/アーカイブ → 直接開く
        navigate(`/browse/${result.node_id}`);
      } else if (result.kind === "pdf" && result.parent_node_id) {
        // PDF → ビューワー直接起動
        navigate(`/browse/${result.parent_node_id}?pdf=${result.node_id}`);
      } else if (result.parent_node_id) {
        // ファイル → 親ディレクトリを開き、対象を選択状態に
        const tab = result.kind === "video" ? "videos" : "images";
        navigate(`/browse/${result.parent_node_id}?tab=${tab}&select=${result.node_id}`);
      }
    },
    [navigate, setQuery],
  );

  // キーボード操作
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!shouldShowDropdown) return;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((prev) => Math.min(prev + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((prev) => Math.max(prev - 1, -1));
    } else if (e.key === "Enter" && activeIndex >= 0) {
      e.preventDefault();
      handleSelect(results[activeIndex]);
    } else if (e.key === "Escape") {
      setIsOpen(false);
      inputRef.current?.blur();
    }
  };

  return (
    <div ref={containerRef} className="relative">
      <div className="flex flex-col gap-1.5">
        <div className="relative">
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            onFocus={() => {
              if (debouncedQuery.length >= 2) setIsOpen(true);
            }}
            placeholder={effectiveScope ? "このフォルダ内を検索..." : "全体を検索..."}
            aria-label="検索"
            data-testid="search-input"
            className="w-full rounded-lg bg-surface-ground py-2 pl-4 pr-4 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>
        <div className="flex gap-1">
          {KIND_FILTERS.map((filter) => (
            <button
              key={filter.label}
              type="button"
              onClick={() => setKind(filter.value)}
              data-testid={`kind-filter-${filter.value ?? "all"}`}
              className={`rounded px-2.5 py-1 text-sm ${
                kind === filter.value
                  ? "bg-blue-600 text-white"
                  : "bg-surface-raised text-gray-400 hover:bg-surface-overlay"
              }`}
            >
              {filter.label}
            </button>
          ))}
          {/* スコープ切替: scope プロップがある場合のみ表示 */}
          {scope && (
            <button
              type="button"
              onClick={() => setIsScopeActive((prev) => !prev)}
              data-testid="scope-toggle"
              title={isScopeActive ? "このフォルダ内を検索中" : "全体を検索中"}
              className={`ml-auto rounded px-2.5 py-1 text-sm ${
                isScopeActive
                  ? "bg-green-600 text-white"
                  : "bg-surface-raised text-gray-400 hover:bg-surface-overlay"
              }`}
            >
              {isScopeActive ? "フォルダ" : "全体"}
            </button>
          )}
        </div>
      </div>
      {shouldShowDropdown && (
        <SearchResults
          results={results}
          hasMore={hasMore}
          isLoading={isLoading}
          isIndexing={isIndexing}
          isError={isError}
          activeIndex={activeIndex}
          onSelect={handleSelect}
          onRetry={refetch}
        />
      )}
    </div>
  );
}
