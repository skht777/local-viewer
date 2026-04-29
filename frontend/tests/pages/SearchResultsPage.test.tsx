// SearchResultsPage の onOpenViewer 配線テスト
// - directory/archive 結果カードで Space → useOpenViewerFromEntry が呼ばれる
// - buildOrigin で /search origin を保存

// pdfjs-dist は jsdom で DOMMatrix 未定義のためモック
vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(() => ({ promise: new Promise(() => {}), destroy: vi.fn() })),
}));

import { screen, waitFor, fireEvent } from "@testing-library/react";
import { Route, Routes } from "react-router-dom";
import SearchResultsPage from "../../src/pages/SearchResultsPage";
import { useViewerStore } from "../../src/stores/viewerStore";
import { renderWithProviders } from "../helpers/renderWithProviders";
import type { ResolvedTarget } from "../../src/utils/resolveFirstViewable";

// resolveFirstViewable のモック（first-viewable 成功 / 失敗を切り替える）
const mockResolveFirstViewable =
  vi.fn<(nodeId: string, queryClient: unknown, sort: string) => Promise<ResolvedTarget | null>>();
vi.mock("../../src/utils/resolveFirstViewable", () => ({
  resolveFirstViewable: (...args: [string, unknown, string]) => mockResolveFirstViewable(...args),
}));

// fetchAllBrowsePages のモック（image/archive 経路で呼ばれる）
vi.mock("../../src/hooks/api/browseQueries", async () => {
  const actual = await vi.importActual<typeof import("../../src/hooks/api/browseQueries")>(
    "../../src/hooks/api/browseQueries",
  );
  return {
    ...actual,
    fetchAllBrowsePages: vi.fn(() => Promise.resolve()),
  };
});

// 検索 API レスポンス
const mockSearchResponse = {
  results: [
    {
      node_id: "dir-result-1",
      name: "matched_dir",
      kind: "directory",
      relative_path: "mount-a/matched_dir",
      parent_node_id: "mount-a",
      size_bytes: null,
      mime_type: null,
      child_count: 5,
      modified_at: null,
      preview_node_ids: null,
    },
  ],
  has_more: false,
  next_cursor: null,
  total_count: 1,
};

// scope name 取得用 (browseNodeOptions 経由) と検索 API
function setupFetchMock() {
  globalThis.fetch = vi.fn((url: string | URL | Request) => {
    const urlStr = typeof url === "string" ? url : url.toString();
    if (urlStr.startsWith("/api/search")) {
      return Promise.resolve(Response.json(mockSearchResponse));
    }
    if (urlStr.includes("/thumbnails/batch")) {
      return Promise.resolve(Response.json({ thumbnails: {} }));
    }
    return Promise.resolve(new Response("{}", { status: 404 }));
  }) as typeof fetch;
}

function renderSearchResultsPage(initialEntry: string) {
  return renderWithProviders(
    <Routes>
      <Route path="/search" element={<SearchResultsPage />} />
    </Routes>,
    { initialEntries: [initialEntry] },
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  setupFetchMock();
  useViewerStore.setState({ viewerOrigin: null, viewerTransitionId: 0 });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("SearchResultsPage の ▶/Space 経由 viewer 起動", () => {
  test("directory 結果で Space を押すと useOpenViewerFromEntry 経由で起動し /search origin が保存される", async () => {
    // first-viewable: matched_dir 内の image を返す（典型的なシナリオ）
    mockResolveFirstViewable.mockResolvedValue({
      entry: {
        node_id: "img-inside",
        name: "p.jpg",
        kind: "image",
        size_bytes: 100,
        mime_type: "image/jpeg",
        child_count: null,
        modified_at: null,
        preview_node_ids: null,
      },
      parentNodeId: "dir-result-1",
    });

    renderSearchResultsPage("/search?q=matched");

    // 検索結果カードが表示されるまで待つ
    await waitFor(() => {
      expect(screen.getByText("matched_dir")).toBeInTheDocument();
    });

    // カードをクリックして選択 → ▶ボタンが表示される
    const card = screen.getByTestId("file-card-dir-result-1");
    fireEvent.click(card);

    // ▶ボタンをクリックして onOpen を発火
    const openButton = await screen.findByTestId("action-open-dir-result-1");
    fireEvent.click(openButton);

    // useOpenViewerFromEntry 経由で resolveFirstViewable が呼ばれることを確認
    await waitFor(() => {
      expect(mockResolveFirstViewable).toHaveBeenCalledWith(
        "dir-result-1",
        expect.anything(),
        "name-asc",
      );
    });

    // viewerOrigin が /search になることを確認（fallback 経路ではない）
    await waitFor(() => {
      const origin = useViewerStore.getState().viewerOrigin;
      expect(origin).not.toBeNull();
      expect(origin?.pathname).toBe("/search");
      expect(origin?.search).toContain("q=matched");
    });
  });

  test("first-viewable 解決失敗時は /search origin が保存されない（再発防止）", async () => {
    mockResolveFirstViewable.mockResolvedValue(null);

    renderSearchResultsPage("/search?q=matched");

    await waitFor(() => {
      expect(screen.getByText("matched_dir")).toBeInTheDocument();
    });

    const card = screen.getByTestId("file-card-dir-result-1");
    fireEvent.click(card);
    const openButton = await screen.findByTestId("action-open-dir-result-1");
    fireEvent.click(openButton);

    await waitFor(() => {
      expect(mockResolveFirstViewable).toHaveBeenCalled();
    });

    // first-viewable が null を返した場合、buildOrigin は呼ばれず origin は保存されない
    expect(useViewerStore.getState().viewerOrigin).toBeNull();
  });
});
