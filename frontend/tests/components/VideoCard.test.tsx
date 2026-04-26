import { fireEvent, render, screen } from "@testing-library/react";
import { VideoCard } from "../../src/components/VideoCard";
import type { BrowseEntry } from "../../src/types/api";

const videoEntry: BrowseEntry = {
  node_id: "vid001",
  name: "clip.mp4",
  kind: "video",
  size_bytes: 10485760, // 10MB
  mime_type: "video/mp4",
  child_count: null,
};

describe("VideoCard", () => {
  test("video要素がsrc付きでレンダリングされる", () => {
    const { container } = render(<VideoCard entry={videoEntry} />);
    const video = container.querySelector("video");
    expect(video).not.toBeNull();
    expect(video?.getAttribute("src")).toBe("/api/file/vid001");
    expect(video?.hasAttribute("controls")).toBe(true);
  });

  test("ファイル名が表示される", () => {
    render(<VideoCard entry={videoEntry} />);
    expect(screen.getByText("clip.mp4")).toBeTruthy();
  });

  test("ファイルサイズが表示される", () => {
    render(<VideoCard entry={videoEntry} />);
    expect(screen.getByText("10.0 MB")).toBeTruthy();
  });

  test("initialTimeが設定されるとloadedmetadata時にcurrentTimeが設定される", () => {
    const { container } = render(<VideoCard entry={videoEntry} initialTime={42.5} />);
    const video = container.querySelector("video") as HTMLVideoElement;
    expect(video).not.toBeNull();

    // loadedmetadata を発火
    fireEvent.loadedMetadata(video);

    expect(video.currentTime).toBe(42.5);
  });

  test("onTimeUpdateがスロットリング付きで呼ばれる", () => {
    const handleTimeUpdate = vi.fn();
    const { container } = render(<VideoCard entry={videoEntry} onTimeUpdate={handleTimeUpdate} />);
    const video = container.querySelector("video") as HTMLVideoElement;

    // currentTime を設定してから timeupdate を発火
    Object.defineProperty(video, "currentTime", { value: 10, writable: true });
    fireEvent.timeUpdate(video);

    expect(handleTimeUpdate).toHaveBeenCalledWith("vid001", 10);
  });

  test("エラー時にフォールバックメッセージが表示される", () => {
    const { container } = render(<VideoCard entry={videoEntry} />);
    const video = container.querySelector("video");
    expect(video).not.toBeNull();

    // onError を発火
    fireEvent.error(video!);

    // video が消えてフォールバックメッセージが表示される
    expect(container.querySelector("video")).toBeNull();
    expect(screen.getByText("この形式はブラウザで再生できません")).toBeTruthy();
  });
});
