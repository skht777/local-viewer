import { render, screen } from "@testing-library/react";
import { FileCard } from "../../src/components/FileCard";
import type { BrowseEntry } from "../../src/types/api";

const imageEntry: BrowseEntry = {
  node_id: "img001",
  name: "photo.jpg",
  kind: "image",
  size_bytes: 2048,
  mime_type: "image/jpeg",
  child_count: null,
};

const dirEntry: BrowseEntry = {
  node_id: "dir001",
  name: "folder",
  kind: "directory",
  size_bytes: null,
  mime_type: null,
  child_count: 5,
};

describe("FileCard", () => {
  test("画像エントリでimgタグが表示される", () => {
    render(<FileCard entry={imageEntry} onClick={() => {}} />);
    const img = screen.getByRole("img");
    expect(img).toHaveAttribute("src", "/api/file/img001");
  });

  test("ディレクトリエントリでアイコンが表示される", () => {
    render(<FileCard entry={dirEntry} onClick={() => {}} />);
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText("folder")).toBeInTheDocument();
  });

  test("ファイルサイズが表示される", () => {
    render(<FileCard entry={imageEntry} onClick={() => {}} />);
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
  });

  test("isSelected=true で aria-current='true' が設定される", () => {
    render(<FileCard entry={imageEntry} onClick={() => {}} isSelected />);
    const button = screen.getByTestId("file-card-img001");
    expect(button).toHaveAttribute("aria-current", "true");
  });

  test("isSelected=false で aria-current が設定されない", () => {
    render(<FileCard entry={imageEntry} onClick={() => {}} />);
    const button = screen.getByTestId("file-card-img001");
    expect(button).not.toHaveAttribute("aria-current");
  });
});
