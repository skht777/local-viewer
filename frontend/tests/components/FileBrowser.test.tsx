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
];

describe("FileBrowser", () => {
  test("エントリがグリッド表示される", () => {
    render(
      <FileBrowser entries={mockEntries} isLoading={false} onNavigate={() => {}} />,
    );
    expect(screen.getByText("photos")).toBeInTheDocument();
    expect(screen.getByText("image.jpg")).toBeInTheDocument();
  });

  test("ローディング中にメッセージが表示される", () => {
    render(
      <FileBrowser entries={[]} isLoading={true} onNavigate={() => {}} />,
    );
    expect(screen.getByText("読み込み中...")).toBeInTheDocument();
  });

  test("エントリが0件で空状態メッセージが表示される", () => {
    render(
      <FileBrowser entries={[]} isLoading={false} onNavigate={() => {}} />,
    );
    expect(screen.getByText("ファイルがありません")).toBeInTheDocument();
  });
});
