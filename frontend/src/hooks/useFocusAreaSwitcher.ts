// ファイルブラウザー ↔ ツリーのフォーカスエリア切替
// - focusArea: "browser" | "tree"
// - handleFocusTree: ツリーへフォーカス（現在ノード優先 → 先頭ノード fallback）
// - handleFocusBrowser: ブラウザーへフォーカス（aria-current="true" のカードへ）

import { useCallback, useState } from "react";

type FocusArea = "browser" | "tree";

interface UseFocusAreaSwitcherParams {
  treeRef: React.RefObject<HTMLElement | null>;
  nodeId: string | undefined;
}

interface UseFocusAreaSwitcherResult {
  focusArea: FocusArea;
  handleFocusTree: () => void;
  handleFocusBrowser: () => void;
}

export function useFocusAreaSwitcher({
  treeRef,
  nodeId,
}: UseFocusAreaSwitcherParams): UseFocusAreaSwitcherResult {
  const [focusArea, setFocusArea] = useState<FocusArea>("browser");

  const handleFocusTree = useCallback(() => {
    setFocusArea("tree");
    const activeNode = treeRef.current?.querySelector<HTMLElement>(`[data-node-id="${nodeId}"]`);
    const fallback = treeRef.current?.querySelector<HTMLElement>("[data-testid^='tree-node-']");
    (activeNode ?? fallback)?.focus();
  }, [treeRef, nodeId]);

  const handleFocusBrowser = useCallback(() => {
    setFocusArea("browser");
    const selectedCard = document.querySelector<HTMLElement>("[aria-current='true']");
    selectedCard?.focus();
  }, []);

  return { focusArea, handleFocusTree, handleFocusBrowser };
}
