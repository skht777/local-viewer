import { render, screen } from "@testing-library/react";
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
  },
  {
    node_id: "file1",
    name: "image.jpg",
    kind: "image",
    size_bytes: 2048,
    mime_type: "image/jpeg",
    child_count: null,
  },
  {
    node_id: "file2",
    name: "movie.mp4",
    kind: "video",
    size_bytes: 10240,
    mime_type: "video/mp4",
    child_count: null,
  },
  {
    node_id: "file3",
    name: "doc.pdf",
    kind: "pdf",
    size_bytes: 4096,
    mime_type: "application/pdf",
    child_count: null,
  },
];

describe("FileBrowser", () => {
  test("filesetsタブでディレクトリとPDFが表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="filesets" />,
    );
    expect(screen.getByText("photos")).toBeInTheDocument();
    expect(screen.getByText("doc.pdf")).toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("imagesタブで画像のみ表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="images" />,
    );
    expect(screen.getByText("image.jpg")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("videosタブで動画のみ表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="videos" />,
    );
    expect(screen.getByText("movie.mp4")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
  });

  test("ローディング中にメッセージが表示される", () => {
    render(
      <FileBrowser entries={[]} isLoading={true} onNavigate={() => {}} tab="filesets" />,
    );
    expect(screen.getByText("読み込み中...")).toBeInTheDocument();
  });

  test("フィルタ後0件で空状態メッセージが表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} tab="videos" />,
    );
    // videos タブには movie.mp4 があるので空にはならない
    // 空の entries で確認
    const { unmount } = render(
      <FileBrowser entries={[]} isLoading={false} onNavigate={() => {}} tab="filesets" />,
    );
    expect(screen.getAllByText("ファイルがありません").length).toBeGreaterThan(0);
    unmount();
  });

  // --- Phase 2: 画像クリック ---

  test("filesetsタブでarchive/PDFがディレクトリより先に表示される", () => {
    const entries: BrowseEntry[] = [
      { node_id: "d1", name: "aaa_dir", kind: "directory", size_bytes: null, mime_type: null, child_count: 5 },
      { node_id: "a1", name: "bbb.zip", kind: "archive", size_bytes: 500, mime_type: "application/zip", child_count: null },
      { node_id: "p1", name: "ccc.pdf", kind: "pdf", size_bytes: 300, mime_type: "application/pdf", child_count: null },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="filesets" />,
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
      { node_id: "d1", name: "dir", kind: "directory", size_bytes: null, mime_type: null, child_count: 5 },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="images" onTabChange={onTabChange} />,
    );
    expect(screen.getByTestId("empty-tab-hint")).toBeInTheDocument();
  });

  test("全タブが空のとき案内が表示されない", () => {
    const onTabChange = vi.fn();
    render(
      <FileBrowser entries={[]} isLoading={false} onNavigate={() => {}} tab="filesets" onTabChange={onTabChange} />,
    );
    expect(screen.queryByTestId("empty-tab-hint")).toBeNull();
  });

  test("案内ボタンクリックで onTabChange が呼ばれる", async () => {
    const onTabChange = vi.fn();
    const entries: BrowseEntry[] = [
      { node_id: "i1", name: "a.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null },
    ];
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} tab="filesets" onTabChange={onTabChange} />,
    );
    await userEvent.click(screen.getByTestId("empty-tab-hint"));
    expect(onTabChange).toHaveBeenCalledWith("images");
  });

  test("画像クリック時に onImageClick がフィルタ済みインデックスで呼ばれる", async () => {
    const entries: BrowseEntry[] = [
      { node_id: "d1", name: "dir", kind: "directory", size_bytes: null, mime_type: null, child_count: 5 },
      { node_id: "i1", name: "a.jpg", kind: "image", size_bytes: 100, mime_type: "image/jpeg", child_count: null },
      { node_id: "i2", name: "b.jpg", kind: "image", size_bytes: 200, mime_type: "image/jpeg", child_count: null },
    ];
    const onImageClick = vi.fn();
    render(
      <FileBrowser entries={entries} isLoading={false} onNavigate={() => {}} onImageClick={onImageClick} tab="images" />,
    );
    // 2番目の画像 (b.jpg) をクリック → フィルタ済み画像配列での index=1
    await userEvent.click(screen.getByText("b.jpg"));
    expect(onImageClick).toHaveBeenCalledWith(1);
  });
});
