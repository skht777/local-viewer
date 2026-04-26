import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { FileBrowser } from "../../src/components/FileBrowser";
import type { BrowseEntry } from "../../src/types/api";

const testQueryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function renderWithQuery(ui: ReactElement) {
  return render(<QueryClientProvider client={testQueryClient}>{ui}</QueryClientProvider>);
}

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
  GlobalWorkerOptions: { workerSrc: "" },
}));

const mockEntries: BrowseEntry[] = [
  {
    node_id: "dir1",
    name: "photos",
    kind: "directory",
    size_bytes: null,
    mime_type: null,
    child_count: 10,
    modified_at: 1_700_000_000,
    preview_node_ids: null,
  },
  {
    node_id: "file1",
    name: "image.jpg",
    kind: "image",
    size_bytes: 2048,
    mime_type: "image/jpeg",
    child_count: null,
    modified_at: 1_700_000_100,
    preview_node_ids: null,
  },
  {
    node_id: "file2",
    name: "movie.mp4",
    kind: "video",
    size_bytes: 10_240,
    mime_type: "video/mp4",
    child_count: null,
    modified_at: 1_700_000_200,
    preview_node_ids: null,
  },
  {
    node_id: "file3",
    name: "doc.pdf",
    kind: "pdf",
    size_bytes: 4096,
    mime_type: "application/pdf",
    child_count: null,
    modified_at: 1_700_000_300,
    preview_node_ids: null,
  },
];

describe("FileBrowser", () => {
  test("filesetsタブでディレクトリとPDFが表示される", () => {
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
    const { unmount } = renderWithQuery(
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
      {
        node_id: "d1",
        name: "aaa_dir",
        kind: "directory",
        size_bytes: null,
        mime_type: null,
        child_count: 5,
        modified_at: 1_700_000_000,
        preview_node_ids: null,
      },
      {
        node_id: "a1",
        name: "bbb.zip",
        kind: "archive",
        size_bytes: 500,
        mime_type: "application/zip",
        child_count: null,
        modified_at: 1_700_000_100,
        preview_node_ids: null,
      },
      {
        node_id: "p1",
        name: "ccc.pdf",
        kind: "pdf",
        size_bytes: 300,
        mime_type: "application/pdf",
        child_count: null,
        modified_at: 1_700_000_200,
        preview_node_ids: null,
      },
    ];
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
      {
        node_id: "d1",
        name: "dir",
        kind: "directory",
        size_bytes: null,
        mime_type: null,
        child_count: 5,
        modified_at: 1_700_000_000,
        preview_node_ids: null,
      },
      {
        node_id: "i1",
        name: "a.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 1_700_000_100,
        preview_node_ids: null,
      },
      {
        node_id: "i2",
        name: "b.jpg",
        kind: "image",
        size_bytes: 200,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 1_700_000_200,
        preview_node_ids: null,
      },
    ];
    const onImageClick = vi.fn();
    renderWithQuery(
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
      {
        node_id: "i2",
        name: "new.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 3000,
        preview_node_ids: null,
      },
      {
        node_id: "i3",
        name: "mid.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 2000,
        preview_node_ids: null,
      },
      {
        node_id: "i1",
        name: "old.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 1000,
        preview_node_ids: null,
      },
    ];
    renderWithQuery(
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
      {
        node_id: "i2",
        name: "old.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 1000,
        preview_node_ids: null,
      },
      {
        node_id: "i1",
        name: "new.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 3000,
        preview_node_ids: null,
      },
    ];
    renderWithQuery(
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
      {
        node_id: "i2",
        name: "beta.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 2000,
        preview_node_ids: null,
      },
      {
        node_id: "i1",
        name: "alpha.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 1000,
        preview_node_ids: null,
      },
    ];
    renderWithQuery(
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
      {
        node_id: "f1",
        name: "aaa.pdf",
        kind: "pdf",
        size_bytes: 100,
        mime_type: "application/pdf",
        child_count: null,
        modified_at: 1000,
        preview_node_ids: null,
      },
      {
        node_id: "d1",
        name: "bbb_dir",
        kind: "directory",
        size_bytes: null,
        mime_type: null,
        child_count: 5,
        modified_at: 2000,
        preview_node_ids: null,
      },
    ];
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
      {
        node_id: "i2",
        name: "has-date.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: 1000,
        preview_node_ids: null,
      },
      {
        node_id: "i1",
        name: "no-date.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: null,
        preview_node_ids: null,
      },
    ];
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
      {
        node_id: "a1",
        name: "photos.zip",
        kind: "archive",
        size_bytes: 500,
        mime_type: "application/zip",
        child_count: null,
        modified_at: 1_700_000_000,
        preview_node_ids: null,
      },
    ];
    const onOpenViewer = vi.fn();
    renderWithQuery(
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
    renderWithQuery(
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

  test("isErrorがtrueの場合にonLoadMoreが発火しない", async () => {
    const onLoadMore = vi.fn();
    // IntersectionObserver をモック: observe 時に isIntersecting: true で即座にコールバック
    const originalIO = globalThis.IntersectionObserver;
    let capturedCallback: IntersectionObserverCallback | null = null;
    globalThis.IntersectionObserver = class MockIO {
      // IntersectionObserver の API シグネチャはコールバックベースのため async/await 不可
      // oxlint-disable-next-line promise/prefer-await-to-callbacks
      constructor(callback: IntersectionObserverCallback) {
        capturedCallback = callback;
      }
      observe() {
        capturedCallback?.(
          [{ isIntersecting: true } as IntersectionObserverEntry],
          this as unknown as IntersectionObserver,
        );
      }
      disconnect() {}
      unobserve() {}
      takeRecords() {
        return [];
      }
      root = null;
      rootMargin = "";
      thresholds = [];
    } as unknown as typeof IntersectionObserver;

    renderWithQuery(
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

    globalThis.IntersectionObserver = originalIO;
  });

  test("ディレクトリの進入ボタンでonNavigateが呼ばれる", async () => {
    const onNavigate = vi.fn();
    renderWithQuery(
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
    renderWithQuery(
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
    renderWithQuery(
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
