// DirectoryTree の className props (mobile ドロワー化対応) テスト

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { DirectoryTree } from "../../src/components/DirectoryTree";
import type { BrowseEntry } from "../../src/types/api";

Element.prototype.scrollIntoView = vi.fn();

vi.mock("../../src/hooks/api/browseQueries", () => ({
  browseNodeOptions: (nodeId: string) => ({
    queryKey: ["browse", nodeId],
    queryFn: () => Promise.resolve({ entries: [] }),
  }),
}));

function entry(name: string): BrowseEntry {
  return {
    node_id: `id-${name}`,
    name,
    kind: "directory",
    size_bytes: null,
    mime_type: null,
    child_count: 0,
    modified_at: null,
    preview_node_ids: null,
  };
}

function renderTree(className?: string) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <DirectoryTree
        rootEntries={[entry("photos")]}
        activeNodeId=""
        ancestorNodeIds={[]}
        onNavigate={vi.fn()}
        className={className}
      />
    </QueryClientProvider>,
  );
}

describe("DirectoryTree responsive", () => {
  test("className 未指定時は既定の w-64 shrink-0 が aside に付与される", () => {
    renderTree();
    const aside = screen.getByTestId("directory-tree");
    expect(aside.className).toContain("w-64");
    expect(aside.className).toContain("shrink-0");
  });

  test("className を指定すると既定が上書きされ、mobile ドロワーのクラスが適用される", () => {
    renderTree("fixed inset-y-0 left-0 z-40 w-72 lg:relative lg:z-auto lg:w-64 lg:shrink-0");
    const aside = screen.getByTestId("directory-tree");
    expect(aside.className).toContain("fixed");
    expect(aside.className).toContain("z-40");
    expect(aside.className).toContain("w-72");
    expect(aside.className).toContain("lg:relative");
    expect(aside.className).toContain("lg:w-64");
    // 既定の w-64 shrink-0 (raw クラス) は付与されない (上書きされている)
    expect(aside.className).not.toMatch(/(^|\s)w-64(\s|$)/);
    expect(aside.className).not.toMatch(/(^|\s)shrink-0(\s|$)/);
  });
});
