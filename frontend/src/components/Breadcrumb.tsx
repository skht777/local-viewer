// パンくずリスト
// - ancestors (祖先ノード) をクリック可能なボタンで表示
// - 現在のディレクトリ名を末尾に plain text で表示
// - URL 組み立ては呼び出し元 (BrowsePage) の onSelect に委譲

import { Fragment, useEffect, useRef } from "react";
import type { AncestorEntry } from "../types/api";

interface BreadcrumbProps {
  ancestors: AncestorEntry[];
  currentName: string;
  onSelect: (nodeId: string) => void;
}

export function Breadcrumb({ ancestors, currentName, onSelect }: BreadcrumbProps) {
  const navRef = useRef<HTMLElement>(null);

  // ancestors 変更時に右端（現在位置）にスクロール
  useEffect(() => {
    if (navRef.current) {
      navRef.current.scrollLeft = navRef.current.scrollWidth;
    }
  }, [ancestors, currentName]);

  return (
    <nav ref={navRef} className="flex min-w-0 items-center overflow-x-auto text-sm">
      {ancestors.map((ancestor, i) => (
        <Fragment key={ancestor.node_id}>
          {i > 0 && (
            <span data-testid="breadcrumb-separator" className="mx-1 shrink-0 text-gray-600">
              /
            </span>
          )}
          <button
            type="button"
            onClick={() => onSelect(ancestor.node_id)}
            className="shrink-0 text-gray-400 transition-colors hover:text-white"
          >
            {ancestor.name}
          </button>
        </Fragment>
      ))}
      {ancestors.length > 0 && (
        <span data-testid="breadcrumb-separator" className="mx-1 shrink-0 text-gray-600">
          /
        </span>
      )}
      <span className="shrink-0 text-gray-200">{currentName}</span>
    </nav>
  );
}
