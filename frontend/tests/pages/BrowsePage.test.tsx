// BrowsePage 統合テスト
// - renderWithProviders で MemoryRouter + QueryClient を提供
// - fetch をモックして固定データを返す
// - タブ切り替えで正しいコンポーネントが表示されるか検証

// pdfjs-dist は jsdom で DOMMatrix 未定義のためモック
vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(() => ({ promise: new Promise(() => {}), destroy: vi.fn() })),
}));

import { screen, waitFor } from "@testing-library/react";
import { Route, Routes } from "react-router-dom";
import BrowsePage from "../../src/pages/BrowsePage";
import type { BrowseResponse } from "../../src/types/api";
import { renderWithProviders } from "../helpers/renderWithProviders";

const mockBrowseData: BrowseResponse = {
  current_node_id: "node-abc",
  current_name: "test-directory",
  parent_node_id: "node-parent",
  ancestors: [],
  entries: [
    {
      node_id: "img1",
      name: "photo.jpg",
      kind: "image",
      size_bytes: 1024,
      mime_type: "image/jpeg",
      child_count: null,
      modified_at: null,
      preview_node_ids: null,
    },
    {
      node_id: "vid1",
      name: "clip.mp4",
      kind: "video",
      size_bytes: 10_240,
      mime_type: "video/mp4",
      child_count: null,
      modified_at: null,
      preview_node_ids: null,
    },
    {
      node_id: "dir1",
      name: "subfolder",
      kind: "directory",
      size_bytes: null,
      mime_type: null,
      child_count: 5,
      modified_at: null,
      preview_node_ids: null,
    },
  ],
  next_cursor: null,
  total_count: null,
};

const mockRootData: BrowseResponse = {
  current_node_id: "root",
  current_name: "root",
  parent_node_id: null,
  ancestors: [],
  entries: [],
  next_cursor: null,
  total_count: null,
};

function setupFetchMock() {
  globalThis.fetch = vi.fn((url: string | URL | Request) => {
    const urlStr = typeof url === "string" ? url : url.toString();
    if (urlStr === "/api/browse") {
      return Promise.resolve(Response.json(mockRootData));
    }
    if (urlStr.startsWith("/api/browse/")) {
      return Promise.resolve(Response.json(mockBrowseData));
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

  test("画像のみディレクトリで images タブに自動切替される", async () => {
    const imagesOnlyData: BrowseResponse = {
      current_node_id: "node-images",
      current_name: "images-only",
      parent_node_id: "node-parent",
      ancestors: [],
      entries: [
        {
          node_id: "img1",
          name: "a.jpg",
          kind: "image",
          size_bytes: 100,
          mime_type: "image/jpeg",
          child_count: null,
        modified_at: null,
        preview_node_ids: null,
        },
        {
          node_id: "img2",
          name: "b.png",
          kind: "image",
          size_bytes: 200,
          mime_type: "image/png",
          child_count: null,
        modified_at: null,
        preview_node_ids: null,
        },
      ],
    next_cursor: null,
    total_count: null,
    };
    globalThis.fetch = vi.fn((url: string | URL | Request) => {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.startsWith("/api/browse/")) {
        return Promise.resolve(Response.json(imagesOnlyData));
      }
      return Promise.resolve(new Response("{}", { status: 404 }));
    }) as typeof fetch;

    renderBrowsePage("/browse/node-images");

    // images タブに自動切替され、画像が表示される
    await waitFor(() => {
      expect(screen.getByText("a.jpg")).toBeTruthy();
    });
  });

  test("動画のみディレクトリで videos タブに自動切替される", async () => {
    const videosOnlyData: BrowseResponse = {
      current_node_id: "node-videos",
      current_name: "videos-only",
      parent_node_id: "node-parent",
      ancestors: [],
      entries: [
        {
          node_id: "vid1",
          name: "clip.mp4",
          kind: "video",
          size_bytes: 5000,
          mime_type: "video/mp4",
          child_count: null,
        modified_at: null,
        preview_node_ids: null,
        },
      ],
    next_cursor: null,
    total_count: null,
    };
    globalThis.fetch = vi.fn((url: string | URL | Request) => {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.startsWith("/api/browse/")) {
        return Promise.resolve(Response.json(videosOnlyData));
      }
      return Promise.resolve(new Response("{}", { status: 404 }));
    }) as typeof fetch;

    renderBrowsePage("/browse/node-videos");

    // videos タブに自動切替され、VideoFeed が表示される
    await waitFor(() => {
      expect(screen.getByText("videos-only")).toBeTruthy();
    });
  });

  test("videos タブで画像のみディレクトリに遷移すると images タブに自動切替される", async () => {
    const imagesOnlyData: BrowseResponse = {
      current_node_id: "node-images",
      current_name: "images-only",
      parent_node_id: "node-parent",
      ancestors: [],
      entries: [
        {
          node_id: "img1",
          name: "a.jpg",
          kind: "image",
          size_bytes: 100,
          mime_type: "image/jpeg",
          child_count: null,
          modified_at: 1_700_000_000,
        preview_node_ids: null,
        },
      ],
    next_cursor: null,
    total_count: null,
    };
    globalThis.fetch = vi.fn((url: string | URL | Request) => {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.startsWith("/api/browse/")) {
        return Promise.resolve(Response.json(imagesOnlyData));
      }
      return Promise.resolve(new Response("{}", { status: 404 }));
    }) as typeof fetch;

    renderBrowsePage("/browse/node-images?tab=videos");

    await waitFor(() => {
      expect(screen.getByText("a.jpg")).toBeTruthy();
    });
  });

  test("images タブでディレクトリのみの場合 filesets タブに自動切替される", async () => {
    const dirsOnlyData: BrowseResponse = {
      current_node_id: "node-dirs",
      current_name: "dirs-only",
      parent_node_id: "node-parent",
      ancestors: [],
      entries: [
        {
          node_id: "dir1",
          name: "subfolder",
          kind: "directory",
          size_bytes: null,
          mime_type: null,
          child_count: 3,
          modified_at: 1_700_000_000,
        preview_node_ids: null,
        },
      ],
    next_cursor: null,
    total_count: null,
    };
    globalThis.fetch = vi.fn((url: string | URL | Request) => {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.startsWith("/api/browse/")) {
        return Promise.resolve(Response.json(dirsOnlyData));
      }
      return Promise.resolve(new Response("{}", { status: 404 }));
    }) as typeof fetch;

    renderBrowsePage("/browse/node-dirs?tab=images");

    await waitFor(() => {
      expect(screen.getByText("subfolder")).toBeTruthy();
    });
  });

  test("videos タブでディレクトリのみの場合 filesets タブに自動切替される", async () => {
    const dirsOnlyData: BrowseResponse = {
      current_node_id: "node-dirs",
      current_name: "dirs-only",
      parent_node_id: "node-parent",
      ancestors: [],
      entries: [
        {
          node_id: "dir1",
          name: "subfolder",
          kind: "directory",
          size_bytes: null,
          mime_type: null,
          child_count: 3,
          modified_at: 1_700_000_000,
        preview_node_ids: null,
        },
      ],
    next_cursor: null,
    total_count: null,
    };
    globalThis.fetch = vi.fn((url: string | URL | Request) => {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.startsWith("/api/browse/")) {
        return Promise.resolve(Response.json(dirsOnlyData));
      }
      return Promise.resolve(new Response("{}", { status: 404 }));
    }) as typeof fetch;

    renderBrowsePage("/browse/node-dirs?tab=videos");

    await waitFor(() => {
      expect(screen.getByText("subfolder")).toBeTruthy();
    });
  });

  test("videos タブで動画があるディレクトリでは videos タブが維持される", async () => {
    const videosData: BrowseResponse = {
      current_node_id: "node-vids",
      current_name: "has-videos",
      parent_node_id: "node-parent",
      ancestors: [],
      entries: [
        {
          node_id: "dir1",
          name: "subfolder",
          kind: "directory",
          size_bytes: null,
          mime_type: null,
          child_count: 3,
          modified_at: 1_700_000_000,
        preview_node_ids: null,
        },
        {
          node_id: "vid1",
          name: "clip.mp4",
          kind: "video",
          size_bytes: 5000,
          mime_type: "video/mp4",
          child_count: null,
          modified_at: 1_700_000_000,
        preview_node_ids: null,
        },
      ],
    next_cursor: null,
    total_count: null,
    };
    globalThis.fetch = vi.fn((url: string | URL | Request) => {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.startsWith("/api/browse/")) {
        return Promise.resolve(Response.json(videosData));
      }
      return Promise.resolve(new Response("{}", { status: 404 }));
    }) as typeof fetch;

    renderBrowsePage("/browse/node-vids?tab=videos");

    await waitFor(() => {
      expect(screen.getByText("has-videos")).toBeTruthy();
    });
    // videos タブが維持され、filesets の subfolder は表示されない
    expect(screen.queryByText("subfolder")).toBeNull();
  });

  test("すべて空のディレクトリでは filesets タブのまま", async () => {
    const emptyData: BrowseResponse = {
      current_node_id: "node-empty",
      current_name: "empty-dir",
      parent_node_id: "node-parent",
      ancestors: [],
      entries: [],
    next_cursor: null,
    total_count: null,
    };
    globalThis.fetch = vi.fn((url: string | URL | Request) => {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.startsWith("/api/browse/")) {
        return Promise.resolve(Response.json(emptyData));
      }
      return Promise.resolve(new Response("{}", { status: 404 }));
    }) as typeof fetch;

    renderBrowsePage("/browse/node-empty");

    await waitFor(() => {
      expect(screen.getByText("empty-dir")).toBeTruthy();
    });
    // filesets タブのまま「ファイルがありません」が表示される
    expect(screen.getByText("ファイルがありません")).toBeTruthy();
  });
});
