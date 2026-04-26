import { render, screen } from "@testing-library/react";
import { VideoFeed } from "../../src/components/VideoFeed";
import type { BrowseEntry } from "../../src/types/api";

const makeVideo = (id: string, name: string): BrowseEntry => ({
  node_id: id,
  name,
  kind: "video",
  size_bytes: 1024 * 1024,
  mime_type: "video/mp4",
  child_count: null,
});

describe("VideoFeed", () => {
  test("動画が0件の場合は空メッセージを表示する", () => {
    render(<VideoFeed videos={[]} />);
    expect(screen.getByText("動画がありません")).toBeTruthy();
  });

  test("動画が1件以上の場合はスクロールコンテナがレンダリングされる", () => {
    const videos = [makeVideo("v1", "clip1.mp4"), makeVideo("v2", "clip2.mp4")];
    const { container } = render(<VideoFeed videos={videos} />);
    // 仮想スクロールのコンテナが存在する
    const scrollContainer = container.querySelector(".flex-1.overflow-y-auto");
    expect(scrollContainer).not.toBeNull();
    // 空メッセージは表示されない
    expect(screen.queryByText("動画がありません")).toBeNull();
  });

  test("スクロールコンテナに仮想スクロール用の高さが設定される", () => {
    const videos = [makeVideo("v1", "clip1.mp4")];
    const { container } = render(<VideoFeed videos={videos} />);

    // 仮想スクロールの外側コンテナ (相対位置)
    const inner = container.querySelector(".relative.mx-auto.max-w-4xl");
    expect(inner).not.toBeNull();
    // getTotalSize() による高さが設定されている
    const { style } = inner as HTMLElement;
    expect(style.height).toBeTruthy();
  });
});
