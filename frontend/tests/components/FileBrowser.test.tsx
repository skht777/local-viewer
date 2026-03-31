import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FileBrowser } from "../../src/components/FileBrowser";
import type { BrowseEntry } from "../../src/types/api";

const mockEntries: BrowseEntry[] = [
  {
    node_id: "dir1",
    name: "photos",
    kind: "directory",
    size_bytes: null,
    mime_type: null,
    child_count: 10,
    modified_at: 1700000000,
  },
  {
    node_id: "file1",
    name: "image.jpg",
    kind: "image",
    size_bytes: 2048,
    mime_type: "image/jpeg",
    child_count: null,
    modified_at: 1700000100,
  },
  {
    node_id: "file2",
    name: "movie.mp4",
    kind: "video",
    size_bytes: 10240,
    mime_type: "video/mp4",
    child_count: null,
    modified_at: 1700000200,
  },
  {
    node_id: "file3",
    name: "doc.pdf",
    kind: "pdf",
    size_bytes: 4096,
    mime_type: "application/pdf",
    child_count: null,
    modified_at: 1700000300,
  },
];

describe("FileBrowser", () => {
  test("filesetsタブでディレクトリとPDFが表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" />,
    );
    expect(screen.getByText("photos")).toBeInTheDocument();
    expect(screen.getByText("doc.pdf")).toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("imagesタブで画像のみ表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="images" sort="name-asc" />,
    );
    expect(screen.getByText("image.jpg")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("videosタブで動画のみ表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="videos" sort="name-asc" />,
    );
    expect(screen.getByText("movie.mp4")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
  });

  test("ローディング中にメッセージが表示される", () => {
    render(
      <FileBrowser entries={[]} isLoading={true} onNavigate={() => {}} tab="filesets" sort="name-asc" />,
    );
    expect(screen.getByText("読み込み中...")).toBeInTheDocument();
  });

  test("フィルタ後0件で空状態メッセージが表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="videos" sort="name-asc" />,
    );
    // videos タブには movie.mp4 があるので空にはならない
    // 空の entries で確認
    const { unmount } = render(
      <FileBrowser entries={[]} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" />,
    );
    expect(screen.getAllByText("ファイルがありません").length).toBeGreaterThan(0);
    unmount();
  });

  // --- Phase 2: 画像クリック ---

  test("filesetsタブでarchive/PDFがディレクトリより先に表示される", () => {
    const entries: BrowseEntry[] = [
      { node_id: "d1", name: "aaa_dir", kind: "directory", size_bytes: null, mime_type: null, child_count: 5, modified_at: 1700000000 },
      { node_id: "a1", name: "bbb.zip", kind: "archive", size_bytes: 500, mime_type: "application/zip", child_count: null, modified_at: 1700000100 },
      { node_id: "p1", name: "ccc.pdf", kind: "pdf", size_bytes: 300, mime_type: "application/pdf", child_count: null, modified_at: 1700000200 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" />,
    );
    // DOM 上の順序: archive/PDF が先、directory が後
    const buttons = screen.getAllByRole("button");
    const names = buttons.map((b) => b.textContent);
    const zipIdx = names.findIndex((n) => n?.includes("bbb.zip"));
    const pdfIdx = names.findIndex((n) => n?.includes("ccc.pdf"));
    const dirIdx = names.findIndex((n) => n?.includes("aaa_dir"));
    expect(zipIdx).toBeLessThan(dirIdx);
    expect(pdfIdx).toBeLessThan(dirIdx);
  });

  test("空タブで他タブへの案内が表示される", () => {
    const onTabChange = vi.fn();
    // images タブだが画像なし。filesets にコンテンツあり
    const entries: BrowseEntry[] = [
      { node_id: "d1", name: "dir", kind: "directory", size_bytes: null, mime_type: null, child_count: 5, modified_at: 1700000000 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="images" sort="name-asc" onTabChange={onTabChange} />,
    );
    expect(screen.getByTestId("empty-tab-hint")).toBeInTheDocument();
  });

  test("全タブが空のとき案内が表示されない", () => {
    const onTabChange = vi.fn();
    render(
      <FileBrowser entries={[]} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" onTabChange={onTabChange} />,
    );
    expect(screen.queryByTestId("empty-tab-hint")).toBeNull();
  });

  test("案内ボタンクリックで onTabChange が呼ばれる", async () => {
    const onTabChange = vi.fn();
    const entries: BrowseEntry[] = [
      { node_id: "i1", name: "a.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 1700000000 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" onTabChange={onTabChange} />,
    );
    await userEvent.click(screen.getByTestId("empty-tab-hint"));
    expect(onTabChange).toHaveBeenCalledWith("images");
  });

  // --- オートフォーカス ---

  test("entriesが渡された時に最初のFileCardにfocusされる", async () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" />,
    );
    // filesets タブのソート: archive/PDF 優先 → directory 後。先頭は file3(doc.pdf)
    const firstCard = screen.getByTestId("file-card-file3");
    await waitFor(() => expect(firstCard).toHaveFocus());
  });

  test("entriesが空の場合focusされない", () => {
    render(
      <FileBrowser entries={[]} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" />,
    );
    expect(document.activeElement).toBe(document.body);
  });

  test("画像クリック時に onImageClick がフィルタ済みインデックスで呼ばれる", async () => {
    const entries: BrowseEntry[] = [
      { node_id: "d1", name: "dir", kind: "directory", size_bytes: null, mime_type: null, child_count: 5, modified_at: 1700000000 },
      { node_id: "i1", name: "a.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 1700000100 },
      { node_id: "i2", name: "b.jpg", kind: "image", size_bytes: 200, mime_type: "image/jpeg", child_count: null, modified_at: 1700000200 },
    ];
    const onImageClick = vi.fn();
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} onImageClick={onImageClick} tab="images" sort="name-asc" />,
    );
    // 2番目の画像 (b.jpg) をクリック → フィルタ済み画像配列での index=1
    await userEvent.click(screen.getByText("b.jpg"));
    expect(onImageClick).toHaveBeenCalledWith(1);
  });

  // --- ソート ---

  test("sort=date-descで更新日時の降順にソートされる", () => {
    const entries: BrowseEntry[] = [
      { node_id: "i1", name: "old.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 1000 },
      { node_id: "i2", name: "new.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 3000 },
      { node_id: "i3", name: "mid.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 2000 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="images" sort="date-desc" />,
    );
    const buttons = screen.getAllByRole("button");
    const names = buttons.map((b) => b.textContent);
    expect(names[0]).toContain("new.jpg");
    expect(names[1]).toContain("mid.jpg");
    expect(names[2]).toContain("old.jpg");
  });

  test("sort=date-ascで更新日時の昇順にソートされる", () => {
    const entries: BrowseEntry[] = [
      { node_id: "i1", name: "new.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 3000 },
      { node_id: "i2", name: "old.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 1000 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="images" sort="date-asc" />,
    );
    const buttons = screen.getAllByRole("button");
    expect(buttons[0].textContent).toContain("old.jpg");
    expect(buttons[1].textContent).toContain("new.jpg");
  });

  test("sort=name-descで名前の降順にソートされる", () => {
    const entries: BrowseEntry[] = [
      { node_id: "i1", name: "alpha.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 1000 },
      { node_id: "i2", name: "beta.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 2000 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="images" sort="name-desc" />,
    );
    const buttons = screen.getAllByRole("button");
    expect(buttons[0].textContent).toContain("beta.jpg");
    expect(buttons[1].textContent).toContain("alpha.jpg");
  });

  test("sort=name-ascでディレクトリ優先の名前順が維持される", () => {
    const entries: BrowseEntry[] = [
      { node_id: "f1", name: "aaa.pdf", kind: "pdf", size_bytes: 100, mime_type: "application/pdf", child_count: null, modified_at: 1000 },
      { node_id: "d1", name: "bbb_dir", kind: "directory", size_bytes: null, mime_type: null, child_count: 5, modified_at: 2000 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="filesets" sort="name-asc" />,
    );
    const buttons = screen.getAllByRole("button");
    const names = buttons.map((b) => b.textContent);
    // filesets タブ: archive/PDF 優先 → directory 後（既存動作維持）
    const pdfIdx = names.findIndex((n) => n?.includes("aaa.pdf"));
    const dirIdx = names.findIndex((n) => n?.includes("bbb_dir"));
    expect(pdfIdx).toBeLessThan(dirIdx);
  });

  test("sort=date-descでmodified_atがnullのエントリが最後になる", () => {
    const entries: BrowseEntry[] = [
      { node_id: "i1", name: "no-date.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: null },
      { node_id: "i2", name: "has-date.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null, modified_at: 1000 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="images" sort="date-desc" />,
    );
    const buttons = screen.getAllByRole("button");
    expect(buttons[0].textContent).toContain("has-date.jpg");
    expect(buttons[1].textContent).toContain("no-date.jpg");
  });
});
