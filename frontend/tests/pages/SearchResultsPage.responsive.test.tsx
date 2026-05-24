// SearchResultsPage ヘッダーのレスポンシブテスト
// - KIND_TABS は overflow-x-auto + whitespace-nowrap で横スクロール許容
// - sort select は lg 未満で w-full、lg 以上で w-auto
// - SearchBar コンテナは lg 未満で w-full、lg 以上で max-w-2xl

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(() => ({ promise: new Promise(() => {}), destroy: vi.fn() })),
}));

import { screen, waitFor } from "@testing-library/react";
import { Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import SearchResultsPage from "../../src/pages/SearchResultsPage";
import { useViewerStore } from "../../src/stores/viewerStore";
import { renderWithProviders } from "../helpers/renderWithProviders";

const mockSearchResponse = {
  results: [
    {
      node_id: "img-1",
      name: "sample.jpg",
      kind: "image",
      relative_path: "mount-a/sample.jpg",
      parent_node_id: "mount-a",
      size_bytes: 100,
      mime_type: "image/jpeg",
      child_count: null,
      modified_at: null,
      preview_node_ids: null,
    },
  ],
  has_more: false,
  next_cursor: null,
  total_count: 1,
};

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

beforeEach(() => {
  vi.clearAllMocks();
  setupFetchMock();
  useViewerStore.setState({ viewerOrigin: null, viewerTransitionId: 0 });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("SearchResultsPage responsive header", () => {
  test("KIND_TABS が overflow-x-auto + whitespace-nowrap で横スクロール許容される", async () => {
    renderWithProviders(
      <Routes>
        <Route path="/search" element={<SearchResultsPage />} />
      </Routes>,
      { initialEntries: ["/search?q=sample"] },
    );
    await waitFor(() => {
      expect(screen.getByTestId("search-kind-tabs")).toBeInTheDocument();
    });
    const tabs = screen.getByTestId("search-kind-tabs");
    expect(tabs.className).toContain("overflow-x-auto");
    expect(tabs.className).toContain("whitespace-nowrap");
  });

  test("sort select は w-full + lg:w-auto を持つ (モバイル幅一杯、デスクトップ自然幅)", async () => {
    renderWithProviders(
      <Routes>
        <Route path="/search" element={<SearchResultsPage />} />
      </Routes>,
      { initialEntries: ["/search?q=sample"] },
    );
    await waitFor(() => {
      expect(screen.getByTestId("search-sort-select")).toBeInTheDocument();
    });
    const sortSelect = screen.getByTestId("search-sort-select");
    expect(sortSelect.className).toContain("w-full");
    expect(sortSelect.className).toContain("lg:w-auto");
  });

  test("各 KIND タブが shrink-0 を持ち、py-2 (タッチターゲット) クラスを持つ", async () => {
    renderWithProviders(
      <Routes>
        <Route path="/search" element={<SearchResultsPage />} />
      </Routes>,
      { initialEntries: ["/search?q=sample"] },
    );
    await waitFor(() => {
      expect(screen.getByTestId("search-kind-all")).toBeInTheDocument();
    });
    const allTab = screen.getByTestId("search-kind-all");
    expect(allTab.className).toContain("shrink-0");
    expect(allTab.className).toContain("py-2");
  });
});
