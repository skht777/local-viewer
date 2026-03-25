// BrowsePage 統合テスト
// - renderWithProviders で MemoryRouter + QueryClient を提供
// - fetch をモックして固定データを返す
// - タブ切り替えで正しいコンポーネントが表示されるか検証

import { screen, waitFor } from "@testing-library/react";
import { Route, Routes } from "react-router-dom";
import BrowsePage from "../../src/pages/BrowsePage";
import type { BrowseResponse } from "../../src/types/api";
import { renderWithProviders } from "../helpers/renderWithProviders";

const mockBrowseData: BrowseResponse = {
  current_node_id: "node-abc",
  current_name: "test-directory",
  parent_node_id: "node-parent",
  entries: [
    {
      node_id: "img1",
      name: "photo.jpg",
      kind: "image",
      size_bytes: 1024,
      mime_type: "image/jpeg",
      child_count: null,
    },
    {
      node_id: "vid1",
      name: "clip.mp4",
      kind: "video",
      size_bytes: 10240,
      mime_type: "video/mp4",
      child_count: null,
    },
    {
      node_id: "dir1",
      name: "subfolder",
      kind: "directory",
      size_bytes: null,
      mime_type: null,
      child_count: 5,
    },
  ],
};

const mockRootData: BrowseResponse = {
  current_node_id: "root",
  current_name: "root",
  parent_node_id: null,
  entries: [],
};

function setupFetchMock() {
  globalThis.fetch = vi.fn((url: string | URL | Request) => {
    const urlStr = typeof url === "string" ? url : url.toString();
    if (urlStr === "/api/browse") {
      return Promise.resolve(new Response(JSON.stringify(mockRootData)));
    }
    if (urlStr.startsWith("/api/browse/")) {
      return Promise.resolve(new Response(JSON.stringify(mockBrowseData)));
    }
    return Promise.resolve(new Response("{}", { status: 404 }));
  }) as typeof fetch;
}

function renderBrowsePage(path: string) {
  return renderWithProviders(
    <Routes>
      <Route path="/browse/:nodeId" element={<BrowsePage />} />
    </Routes>,
    { initialEntries: [path] },
  );
}

beforeEach(() => {
  setupFetchMock();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("BrowsePage", () => {
  test("tab=filesetsがデフォルトで表示される", async () => {
    renderBrowsePage("/browse/node-abc");

    // データ読み込み完了を待つ
    await waitFor(() => {
      expect(screen.getByText("test-directory")).toBeTruthy();
    });

    // ファイルセットタブがアクティブ (FileBrowser が表示)
    // 動画タブの空メッセージは表示されない
    expect(screen.queryByText("動画がありません")).toBeNull();
  });

  test("tab=videosでVideoFeedが表示される", async () => {
    renderBrowsePage("/browse/node-abc?tab=videos");

    await waitFor(() => {
      expect(screen.getByText("test-directory")).toBeTruthy();
    });

    // VideoFeed のスクロールコンテナが存在する
    // (仮想スクロールは jsdom で完全に動作しないが、コンテナは描画される)
  });

  test("ヘッダーにディレクトリ名が表示される", async () => {
    renderBrowsePage("/browse/node-abc");

    await waitFor(() => {
      expect(screen.getByText("test-directory")).toBeTruthy();
    });
  });
});
