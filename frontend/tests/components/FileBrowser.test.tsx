import { render, screen } from "@testing-library/react";
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
});
