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
  { label: "\u{1F5BC}", value: "image" },
  { label: "\u{1F3AC}", value: "video" },
  { label: "\u{1F4C4}", value: "pdf" },
  { label: "\u{1F4E6}", value: "archive" },
] as const;

export function SearchBar() {
  const navigate = useNavigate();
  const {
    query,
    setQuery,
    debouncedQuery,
    kind,
    setKind,
    results,
    hasMore,
    isLoading,
    isIndexing,
  } = useSearch();

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
      <div className="flex gap-2">
        <div className="relative flex-1">
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            onFocus={() => {
              if (debouncedQuery.length >= 2) setIsOpen(true);
            }}
            placeholder="検索..."
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
              className={`rounded px-2 py-1 text-sm ${
                kind === filter.value
                  ? "bg-blue-600 text-white"
                  : "bg-surface-raised text-gray-400 hover:bg-surface-overlay"
              }`}
            >
              {filter.label}
            </button>
          ))}
        </div>
      </div>
      {shouldShowDropdown && (
        <SearchResults
          results={results}
          hasMore={hasMore}
          isLoading={isLoading}
          isIndexing={isIndexing}
          activeIndex={activeIndex}
          onSelect={handleSelect}
        />
      )}
    </div>
  );
}
