// SearchBar の遷移ロジック
// - handleSelect: 検索結果クリック / Enter で kind 別の遷移を実施
//   - directory/archive: push 遷移（viewerOrigin 不使用）
//   - PDF: scope ありなら viewerOrigin 設定 + push（B キー閉じで origin に戻れる）
//   - image/video: scope ありなら viewerOrigin 設定 + replace（既存挙動維持）
//   - TopPage 文脈（scope 無し）: push のみ
// - navigateToSearchPage: q.trim() ≥ 2 文字で /search?q=... に遷移

import { useCallback } from "react";
import type { Location, NavigateFunction } from "react-router-dom";
import type { SearchResult } from "../types/api";
import type { useViewerStore } from "../stores/viewerStore";

type SetViewerOrigin = ReturnType<typeof useViewerStore.getState>["setViewerOrigin"];

interface UseSearchNavigationParams {
  scope: string | undefined;
  effectiveScope: string | undefined;
  query: string;
  kind: string | null;
  location: Location;
  navigate: NavigateFunction;
  setViewerOrigin: SetViewerOrigin;
  setQuery: (query: string) => void;
  setIsOpen: (open: boolean) => void;
}

interface UseSearchNavigationResult {
  handleSelect: (result: SearchResult) => void;
  navigateToSearchPage: () => void;
}

const MIN_QUERY_LENGTH = 2;

export function useSearchNavigation({
  scope,
  effectiveScope,
  query,
  kind,
  location,
  navigate,
  setViewerOrigin,
  setQuery,
  setIsOpen,
}: UseSearchNavigationParams): UseSearchNavigationResult {
  const handleSelect = useCallback(
    (result: SearchResult) => {
      setIsOpen(false);
      setQuery("");

      if (result.kind === "directory" || result.kind === "archive") {
        navigate(`/browse/${result.node_id}`);
        return;
      }

      if (!result.parent_node_id) {
        return;
      }

      // 現在 URL から mode/sort を継承（既定値は URL に書かない）
      const current = new URLSearchParams(location.search);
      const target = new URLSearchParams();
      if (result.kind === "pdf") {
        target.set("pdf", result.node_id);
      } else {
        const tab = result.kind === "video" ? "videos" : "images";
        target.set("tab", tab);
        target.set("select", result.node_id);
      }
      const mode = current.get("mode");
      const sort = current.get("sort");
      if (mode) {
        target.set("mode", mode);
      }
      if (sort) {
        target.set("sort", sort);
      }

      const url = `/browse/${result.parent_node_id}?${target}`;

      if (result.kind === "pdf") {
        // PDF viewer 起動: ブラウザバックで呼び出し元に戻れるよう push 化
        if (scope) {
          setViewerOrigin({ pathname: `/browse/${scope}`, search: location.search });
        }
        navigate(url);
      } else if (scope) {
        // Image/video（viewer 起動ではない）: scope 戻り用に origin 設定 + replace（既存挙動維持）
        setViewerOrigin({ pathname: `/browse/${scope}`, search: location.search });
        navigate(url, { replace: true });
      } else {
        // TopPage 文脈: origin 無し、push 遷移
        navigate(url);
      }
    },
    [navigate, setQuery, setIsOpen, location.search, scope, setViewerOrigin],
  );

  // 検索結果一覧ページへの遷移
  // - q.trim() が 2 文字未満なら何もしない
  // - effectiveScope（フォルダ内トグル ON のとき）を URL に保持
  // - kind は外部から設定された URL を保持するため kind パラメータがあれば乗せる
  const navigateToSearchPage = useCallback(() => {
    const trimmed = query.trim();
    if (trimmed.length < MIN_QUERY_LENGTH) {
      return;
    }
    setIsOpen(false);
    const params = new URLSearchParams({ q: trimmed });
    if (effectiveScope) {
      params.set("scope", effectiveScope);
    }
    if (kind) {
      params.set("kind", kind);
    }
    navigate(`/search?${params.toString()}`);
  }, [query, effectiveScope, kind, navigate, setIsOpen]);

  return { handleSelect, navigateToSearchPage };
}
