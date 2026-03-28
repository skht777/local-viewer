import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThumbnailSidebar } from "../../src/components/ThumbnailSidebar";
import type { BrowseEntry } from "../../src/types/api";

// jsdom は scrollIntoView 未実装
beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
});

function makeImage(id: string, name: string): BrowseEntry {
  return { node_id: id, name, kind: "image", size_bytes: 1024, mime_type: "image/jpeg", child_count: null };
}

describe("ThumbnailSidebar", () => {
  const images = [makeImage("a", "img1.jpg"), makeImage("b", "img2.jpg"), makeImage("c", "img3.jpg")];

  test("画像一覧がサムネイルとして表示される", () => {
    render(<ThumbnailSidebar images={images} currentIndex={0} onSelect={() => {}} />);
    const thumbnails = screen.getAllByRole("button");
    expect(thumbnails).toHaveLength(3);
  });

  test("現在の画像がハイライトされる", () => {
    render(<ThumbnailSidebar images={images} currentIndex={1} onSelect={() => {}} />);
    const buttons = screen.getAllByRole("button");
    // 2番目のボタンがアクティブクラスを持つ
    expect(buttons[1].className).toContain("ring");
  });

  test("サムネイルクリックで onSelect が呼ばれる", async () => {
    const onSelect = vi.fn();
    render(<ThumbnailSidebar images={images} currentIndex={0} onSelect={onSelect} />);
    const buttons = screen.getAllByRole("button");
    await userEvent.click(buttons[2]);
    expect(onSelect).toHaveBeenCalledWith(2);
  });

  test("アクティブサムネイルに aria-current=true が設定される", () => {
    render(<ThumbnailSidebar images={images} currentIndex={1} onSelect={() => {}} />);
    const buttons = screen.getAllByRole("button");
    expect(buttons[1]).toHaveAttribute("aria-current", "true");
  });

  test("非アクティブサムネイルに aria-current がない", () => {
    render(<ThumbnailSidebar images={images} currentIndex={1} onSelect={() => {}} />);
    const buttons = screen.getAllByRole("button");
    expect(buttons[0]).not.toHaveAttribute("aria-current", "true");
    expect(buttons[2]).not.toHaveAttribute("aria-current", "true");
  });
});
