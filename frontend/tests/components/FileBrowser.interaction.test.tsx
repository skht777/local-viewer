// FileBrowser インタラクション（クリック・ダブルクリック・キーボード・オーバーレイ・isError）

import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FileBrowser } from "../../src/components/FileBrowser";
import type { BrowseEntry } from "../../src/types/api";
import {
  installMockIntersectionObserver,
  makeArchiveEntry,
  mockEntries,
  renderFileBrowser,
} from "./__helpers__/fileBrowserTestHelpers";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
  GlobalWorkerOptions: { workerSrc: "" },
}));

describe("FileBrowser 選択・ダブルクリック・オーバーレイ", () => {
  test("シングルクリックでカードが選択状態になる", async () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    await userEvent.click(screen.getByTestId("file-card-dir1"));
    expect(screen.getByTestId("file-card-dir1")).toHaveAttribute("aria-current", "true");
  });

  test("ダブルクリックでディレクトリにonNavigateが呼ばれる", async () => {
    const onNavigate = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={onNavigate}
        tab="filesets"
        sort="name-asc"
      />,
    );
    await userEvent.dblClick(screen.getByTestId("file-card-dir1"));
    expect(onNavigate).toHaveBeenCalledWith("dir1");
  });

  test("ダブルクリックで画像にonImageClickが呼ばれる", async () => {
    const onImageClick = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        onImageClick={onImageClick}
        tab="images"
        sort="name-asc"
      />,
    );
    await userEvent.dblClick(screen.getByTestId("file-card-file1"));
    expect(onImageClick).toHaveBeenCalledWith(0);
  });

  test("選択中にEscapeで選択解除される", async () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    // カードを選択
    await userEvent.click(screen.getByTestId("file-card-dir1"));
    expect(screen.getByTestId("file-card-dir1")).toHaveAttribute("aria-current", "true");

    // Escape で選択解除
    await userEvent.keyboard("{Escape}");
    // dir1 の選択が解除される（デフォルトの先頭カードに戻る可能性があるが、dir1 は非選択）
    expect(screen.getByTestId("file-card-dir1")).not.toHaveAttribute("aria-current");
  });

  test("カード外クリックで選択解除される", async () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    // カードを選択
    await userEvent.click(screen.getByTestId("file-card-dir1"));
    expect(screen.getByTestId("file-card-dir1")).toHaveAttribute("aria-current", "true");

    // メインエリアの背景をクリック（カードの外側）
    const main = screen.getByRole("main");
    await userEvent.click(main);
    expect(screen.getByTestId("file-card-dir1")).not.toHaveAttribute("aria-current");
  });

  test("ディレクトリの開くボタンでonOpenViewerが呼ばれる", async () => {
    const onOpenViewer = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        onOpenViewer={onOpenViewer}
        tab="filesets"
        sort="name-asc"
      />,
    );
    // ディレクトリを選択してオーバーレイ表示
    await userEvent.click(screen.getByTestId("file-card-dir1"));
    // 開くボタンクリック
    await userEvent.click(screen.getByTestId("action-open-dir1"));
    expect(onOpenViewer).toHaveBeenCalledWith("dir1");
  });

  test("アーカイブの開くボタンでonOpenViewerが呼ばれる", async () => {
    const entries: BrowseEntry[] = [
      makeArchiveEntry({
        node_id: "a1",
        name: "photos.zip",
        size_bytes: 500,
        modified_at: 1_700_000_000,
      }),
    ];
    const onOpenViewer = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        onOpenViewer={onOpenViewer}
        tab="filesets"
        sort="name-asc"
      />,
    );
    await userEvent.click(screen.getByTestId("file-card-a1"));
    await userEvent.click(screen.getByTestId("action-open-a1"));
    expect(onOpenViewer).toHaveBeenCalledWith("a1");
  });

  // --- isError ガード ---

  test("isErrorがtrueの場合にエラーメッセージが表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
        hasMore={true}
        isError={true}
      />,
    );
    expect(
      screen.getByText("読み込みに失敗しました。ページをリロードしてください。"),
    ).toBeInTheDocument();
  });

  test("isErrorがtrueの場合にonLoadMoreが発火しない", () => {
    const onLoadMore = vi.fn();
    const { restore } = installMockIntersectionObserver();

    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
        hasMore={true}
        isLoadingMore={false}
        isError={true}
        onLoadMore={onLoadMore}
      />,
    );

    // IntersectionObserver が isIntersecting: true で発火しても onLoadMore は呼ばれない
    expect(onLoadMore).not.toHaveBeenCalled();

    restore();
  });

  test("ディレクトリの進入ボタンでonNavigateが呼ばれる", async () => {
    const onNavigate = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={onNavigate}
        tab="filesets"
        sort="name-asc"
      />,
    );
    await userEvent.click(screen.getByTestId("file-card-dir1"));
    await userEvent.click(screen.getByTestId("action-enter-dir1"));
    expect(onNavigate).toHaveBeenCalledWith("dir1");
  });

  test("画像の開くボタンでonImageClickが呼ばれる", async () => {
    const onImageClick = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        onImageClick={onImageClick}
        tab="images"
        sort="name-asc"
      />,
    );
    await userEvent.click(screen.getByTestId("file-card-file1"));
    await userEvent.click(screen.getByTestId("action-open-file1"));
    expect(onImageClick).toHaveBeenCalledWith(0);
  });

  test("PDFの開くボタンでonPdfClickが呼ばれる", async () => {
    const onPdfClick = vi.fn();
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        onPdfClick={onPdfClick}
        tab="filesets"
        sort="name-asc"
      />,
    );
    await userEvent.click(screen.getByTestId("file-card-file3"));
    await userEvent.click(screen.getByTestId("action-open-file3"));
    expect(onPdfClick).toHaveBeenCalledWith("file3");
  });
});
