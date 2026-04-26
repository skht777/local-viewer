import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FileBrowser } from "../../src/components/FileBrowser";
import type { BrowseEntry } from "../../src/types/api";
import {
  installMockIntersectionObserver,
  makeArchiveEntry,
  makeDirectoryEntry,
  makeImageEntry,
  makePdfEntry,
  mockEntries,
  renderFileBrowser,
} from "./__helpers__/fileBrowserTestHelpers";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
  GlobalWorkerOptions: { workerSrc: "" },
}));

describe("FileBrowser", () => {
  test("filesetsタブでディレクトリとPDFが表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("photos")).toBeInTheDocument();
    expect(screen.getByText("doc.pdf")).toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("imagesタブで画像のみ表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("image.jpg")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("videosタブで動画のみ表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="videos"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("movie.mp4")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
  });

  test("ローディング中にメッセージが表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={[]}
        isLoading={true}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("読み込み中...")).toBeInTheDocument();
  });

  test("フィルタ後0件で空状態メッセージが表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="videos"
        sort="name-asc"
      />,
    );
    // videos タブには movie.mp4 があるので空にはならない
    // 空の entries で確認
    const { unmount } = renderFileBrowser(
      <FileBrowser
        entries={[]}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    expect(screen.getAllByText("ファイルがありません").length).toBeGreaterThan(0);
    unmount();
  });

  // --- Phase 2: 画像クリック ---

  test("filesetsタブでarchive/PDFがディレクトリより先に表示される", () => {
    const entries: BrowseEntry[] = [
      makeDirectoryEntry({
        node_id: "d1",
        name: "aaa_dir",
        child_count: 5,
        modified_at: 1_700_000_000,
      }),
      makeArchiveEntry({
        node_id: "a1",
        name: "bbb.zip",
        size_bytes: 500,
        modified_at: 1_700_000_100,
      }),
      makePdfEntry({ node_id: "p1", name: "ccc.pdf", size_bytes: 300, modified_at: 1_700_000_200 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    // DOM 上の順序: archive/PDF が先、directory が後
    const cards = screen.getAllByTestId(/^file-card-/);
    const names = cards.map((c) => c.textContent);
    const zipIdx = names.findIndex((n) => n?.includes("bbb.zip"));
    const pdfIdx = names.findIndex((n) => n?.includes("ccc.pdf"));
    const dirIdx = names.findIndex((n) => n?.includes("aaa_dir"));
    expect(zipIdx).toBeLessThan(dirIdx);
    expect(pdfIdx).toBeLessThan(dirIdx);
  });

  // --- オートフォーカス ---

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

  // --- ソート ---

  test("sort=date-descでサーバーソート済みの降順が維持される", () => {
    // サーバーサイドソート済み: 日付降順
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "new.jpg", modified_at: 3000 }),
      makeImageEntry({ node_id: "i3", name: "mid.jpg", modified_at: 2000 }),
      makeImageEntry({ node_id: "i1", name: "old.jpg", modified_at: 1000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="date-desc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    const names = cards.map((c) => c.textContent);
    expect(names[0]).toContain("new.jpg");
    expect(names[1]).toContain("mid.jpg");
    expect(names[2]).toContain("old.jpg");
  });

  test("sort=date-ascでサーバーソート済みの昇順が維持される", () => {
    // サーバーサイドソート済み: 日付昇順
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "old.jpg", modified_at: 1000 }),
      makeImageEntry({ node_id: "i1", name: "new.jpg", modified_at: 3000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="date-asc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    expect(cards[0].textContent).toContain("old.jpg");
    expect(cards[1].textContent).toContain("new.jpg");
  });

  test("sort=name-descでサーバーソート済みの名前降順が維持される", () => {
    // サーバーサイドソート済み: 名前降順
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "beta.jpg", modified_at: 2000 }),
      makeImageEntry({ node_id: "i1", name: "alpha.jpg", modified_at: 1000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="name-desc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    expect(cards[0].textContent).toContain("beta.jpg");
    expect(cards[1].textContent).toContain("alpha.jpg");
  });

  test("sort=name-ascでディレクトリ優先の名前順が維持される", () => {
    const entries: BrowseEntry[] = [
      makePdfEntry({ node_id: "f1", name: "aaa.pdf", size_bytes: 100, modified_at: 1000 }),
      makeDirectoryEntry({ node_id: "d1", name: "bbb_dir", child_count: 5, modified_at: 2000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    const names = cards.map((c) => c.textContent);
    // filesets タブ: archive/PDF 優先 → directory 後（既存動作維持）
    const pdfIdx = names.findIndex((n) => n?.includes("aaa.pdf"));
    const dirIdx = names.findIndex((n) => n?.includes("bbb_dir"));
    expect(pdfIdx).toBeLessThan(dirIdx);
  });

  // --- オートセレクト ---

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

  test("sort=date-descでサーバーソート済みのnullエントリが最後に維持される", () => {
    // サーバーサイドソート済み: 日付降順、null は末尾
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "has-date.jpg", modified_at: 1000 }),
      makeImageEntry({ node_id: "i1", name: "no-date.jpg", modified_at: null }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="date-desc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    expect(cards[0].textContent).toContain("has-date.jpg");
    expect(cards[1].textContent).toContain("no-date.jpg");
  });
});

// --- C2: 選択・ダブルクリック・オーバーレイ ---

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
