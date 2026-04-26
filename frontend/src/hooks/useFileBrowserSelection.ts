// FileBrowser の選択状態とフォーカス制御
// - クリック選択（localSelectedId）優先 → selectedNodeId プロップ → 先頭エントリ の優先順
// - エントリ変更時にローカル選択リセット + 先頭カードへ自動 focus
// - Escape / 外クリックで選択解除

import type { KeyboardEvent } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { BrowseEntry } from "../types/api";

interface UseFileBrowserSelectionParams {
  filtered: BrowseEntry[];
  selectedNodeId?: string;
}

interface UseFileBrowserSelectionResult {
  effectiveSelectedId: string | null;
  firstCardRef: React.RefObject<HTMLDivElement | null>;
  setLocalSelectedId: (id: string | null) => void;
  handleSelect: (entry: BrowseEntry) => void;
  handleKeyDown: (e: KeyboardEvent) => void;
  handleMainClick: (e: React.MouseEvent) => void;
}

export function useFileBrowserSelection({
  filtered,
  selectedNodeId,
}: UseFileBrowserSelectionParams): UseFileBrowserSelectionResult {
  const firstCardRef = useRef<HTMLDivElement>(null);
  const firstEntryId = filtered[0]?.node_id ?? null;

  const [localSelectedId, setLocalSelectedId] = useState<string | null>(null);
  const effectiveSelectedId = localSelectedId ?? selectedNodeId ?? firstEntryId;

  // entries 変更時にローカル選択をリセット
  useEffect(() => {
    setLocalSelectedId(null);
  }, [firstEntryId]);

  // 先頭エントリへ自動 focus（オートセレクト）
  useEffect(() => {
    if (firstEntryId) {
      firstCardRef.current?.focus();
    }
  }, [firstEntryId]);

  // シングルクリック: カード選択
  const handleSelect = useCallback((entry: BrowseEntry) => {
    setLocalSelectedId(entry.node_id);
  }, []);

  // Escape で選択解除
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "Escape") {
      setLocalSelectedId(null);
    }
  }, []);

  // カード外クリックで選択解除
  const handleMainClick = useCallback((e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      setLocalSelectedId(null);
    }
  }, []);

  return {
    effectiveSelectedId,
    firstCardRef,
    setLocalSelectedId,
    handleSelect,
    handleKeyDown,
    handleMainClick,
  };
}
