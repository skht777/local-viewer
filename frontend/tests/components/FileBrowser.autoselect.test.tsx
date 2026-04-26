// FileBrowser オートセレクト・オートフォーカス・選択優先制御
// - selectedNodeId 未指定時の先頭カード自動選択
// - selectedNodeId 指定時の優先表示
// - 空 entries 時の focus 抑止
// - 画像 dblclick 時のフィルタ済みインデックス計算

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FileBrowser } from "../../src/components/FileBrowser";
import type { BrowseEntry } from "../../src/types/api";
import {
  makeDirectoryEntry,
  makeImageEntry,
  mockEntries,
  renderFileBrowser,
} from "./__helpers__/fileBrowserTestHelpers";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
  GlobalWorkerOptions: { workerSrc: "" },
}));

describe("FileBrowser オートセレクト", () => {
  test("entriesが渡された時に最初のFileCardにfocusされる", async () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    // filesets タブのソート: archive/PDF 優先 → directory 後。先頭は file3(doc.pdf)
    const firstCard = screen.getByTestId("file-card-file3");
    await waitFor(() => expect(firstCard).toHaveFocus());
  });

  test("entriesが空の場合focusされない", () => {
    renderFileBrowser(
      <FileBrowser
        entries={[]}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    expect(document.activeElement).toBe(document.body);
  });

  test("画像ダブルクリック時に onImageClick がフィルタ済みインデックスで呼ばれる", async () => {
    const entries: BrowseEntry[] = [
      makeDirectoryEntry({
        node_id: "d1",
        name: "dir",
        child_count: 5,
        modified_at: 1_700_000_000,
      }),
      makeImageEntry({ node_id: "i1", name: "a.jpg", size_bytes: 100, modified_at: 1_700_000_100 }),
      makeImageEntry({ node_id: "i2", name: "b.jpg", size_bytes: 200, modified_at: 1_700_000_200 }),
    ];
    const onImageClick = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        onImageClick={onImageClick}
        tab="images"
        sort="name-asc"
      />,
    );
    // 2番目の画像 (b.jpg) をダブルクリック → フィルタ済み画像配列での index=1
    await userEvent.dblClick(screen.getByText("b.jpg"));
    expect(onImageClick).toHaveBeenCalledWith(1);
  });

  test("selectedNodeIdが未指定の場合、先頭のFileCardがselected状態になる", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    // filesets タブ: archive/PDF 優先 → 先頭は file3(doc.pdf)
    const firstCard = screen.getByTestId("file-card-file3");
    expect(firstCard).toHaveAttribute("aria-current", "true");
  });

  test("selectedNodeIdが指定されている場合はそちらが優先される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
        selectedNodeId="dir1"
      />,
    );
    expect(screen.getByTestId("file-card-dir1")).toHaveAttribute("aria-current", "true");
    expect(screen.getByTestId("file-card-file3")).not.toHaveAttribute("aria-current");
  });
});
