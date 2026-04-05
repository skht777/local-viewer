// DirectoryTree コンポーネントのテスト
// - ルートエントリの表示 (directory/archive/pdf のみフィルタ)
// - WAI-ARIA TreeView 構造 (role="tree", role="treeitem")
// - クリックで onNavigate コールバック呼び出し
// - ancestorNodeIds による自動展開
// - 空 rootEntries で見出しのみ表示

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { DirectoryTree } from "../../src/components/DirectoryTree";
import { useViewerStore } from "../../src/stores/viewerStore";
import type { BrowseEntry } from "../../src/types/api";

// jsdom に scrollIntoView がないためモック
Element.prototype.scrollIntoView = vi.fn();

// browseNodeOptions をモック (lazy loading 用)
vi.mock("../../src/hooks/api/browseQueries", () => ({
  browseNodeOptions: (nodeId: string) => ({
    queryKey: ["browse", nodeId],
    queryFn: () =>
      Promise.resolve({
        entries: [
          {
            node_id: `${nodeId}-child1`,
            name: "child-dir",
            kind: "directory",
            size_bytes: null,
            mime_type: null,
            child_count: 0,
            modified_at: null,
            preview_node_ids: null,
          },
          {
            node_id: `${nodeId}-img`,
            name: "img.jpg",
            kind: "image",
            size_bytes: 100,
            mime_type: "image/jpeg",
            child_count: null,
            modified_at: null,
            preview_node_ids: null,
          },
        ],
      }),
  }),
}));

function entry(name: string, kind: BrowseEntry["kind"] = "directory"): BrowseEntry {
  return {
    node_id: `id-${name}`,
    name,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: kind === "directory" ? 3 : null,
    modified_at: null,
    preview_node_ids: null,
  };
}

function renderTree(
  rootEntries: BrowseEntry[],
  {
    activeNodeId = "",
    ancestorNodeIds = [] as string[],
    onNavigate = vi.fn(),
  } = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <DirectoryTree
        rootEntries={rootEntries}
        activeNodeId={activeNodeId}
        ancestorNodeIds={ancestorNodeIds}
        onNavigate={onNavigate}
      />
    </QueryClientProvider>,
  );
}

describe("DirectoryTree", () => {
  beforeEach(() => {
    // zustand ストアをリセット（expandedNodeIds を空にする）
    useViewerStore.setState({ expandedNodeIds: new Set<string>() });
  });

  test("ディレクトリ/アーカイブ/PDFのみ表示される", () => {
    const entries = [
      entry("photos"),
      entry("archive.zip", "archive"),
      entry("doc.pdf", "pdf"),
      entry("image.jpg", "image"),
      entry("video.mp4", "video"),
    ];
    renderTree(entries);

    expect(screen.getByText("photos")).toBeInTheDocument();
    expect(screen.getByText("archive.zip")).toBeInTheDocument();
    expect(screen.getByText("doc.pdf")).toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
    expect(screen.queryByText("video.mp4")).not.toBeInTheDocument();
  });

  test("WAI-ARIA tree ロールが設定されている", () => {
    renderTree([entry("photos")]);
    expect(screen.getByRole("tree")).toBeInTheDocument();
    expect(screen.getByRole("treeitem")).toBeInTheDocument();
  });

  test("クリックで onNavigate が呼ばれる", async () => {
    const onNavigate = vi.fn();
    renderTree([entry("photos")], { onNavigate });

    await userEvent.click(screen.getByText("photos"));
    expect(onNavigate).toHaveBeenCalledWith("id-photos");
  });

  test("アクティブノードにハイライトクラスが適用される", () => {
    renderTree([entry("photos"), entry("docs")], {
      activeNodeId: "id-photos",
    });

    const activeButton = screen.getByTestId("tree-node-id-photos");
    expect(activeButton.className).toContain("text-white");
  });

  test("空 rootEntries で見出しのみ表示される", () => {
    renderTree([]);
    expect(screen.getByText("ディレクトリ")).toBeInTheDocument();
    expect(screen.queryByRole("treeitem")).not.toBeInTheDocument();
  });

  test("ancestorNodeIds に含まれるノードが自動展開される", async () => {
    renderTree([entry("photos")], {
      ancestorNodeIds: ["id-photos"],
    });

    // 展開状態: aria-expanded="true"
    const treeItem = screen.getByRole("treeitem");
    expect(treeItem).toHaveAttribute("aria-expanded", "true");
  });

  test("展開されていないノードの子はfetchされない", () => {
    renderTree([entry("photos")]);

    // 展開されていない → aria-expanded="false" + group なし
    const treeItem = screen.getByRole("treeitem");
    expect(treeItem).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("group")).not.toBeInTheDocument();
  });
});
