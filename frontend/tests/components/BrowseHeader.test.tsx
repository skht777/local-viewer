// BrowseHeader レスポンシブテスト
// - lg 以上では SearchBar が右側 1 段に表示 (mobile 用 2 段目は lg:hidden で非表示)
// - lg 未満では 2 段目が表示される (1 段目の右側は hidden lg:flex で非表示)
// - 両方とも DOM には存在 (CSS で表示分岐) するため、container クラスで検証

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, test, vi } from "vitest";
import { BrowseHeader } from "../../src/components/BrowseHeader";

vi.mock("../../src/hooks/useSearch", () => ({
  useSearch: () => ({
    query: "",
    setQuery: vi.fn(),
    debouncedQuery: "",
    kind: null,
    setKind: vi.fn(),
    results: [],
    hasMore: false,
    isLoading: false,
    isError: false,
    isIndexing: false,
    refetch: vi.fn(),
  }),
}));

function renderHeader() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <BrowseHeader
          currentName="photos"
          ancestors={[]}
          onBreadcrumbSelect={vi.fn()}
          mode="cg"
          onModeChange={vi.fn()}
          nodeId="some-node"
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("BrowseHeader responsive", () => {
  test("ハンバーガーと戻るボタンが表示される", () => {
    renderHeader();
    expect(screen.getByTestId("sidebar-toggle")).toBeInTheDocument();
    expect(screen.getByText("← トップ")).toBeInTheDocument();
  });

  test("lg 以上の SearchBar コンテナに hidden lg:flex が付与される", () => {
    const { container } = renderHeader();
    // 1 段目内側の lg 以上用ラッパーは hidden lg:flex
    const desktopOnly = container.querySelector(".hidden.lg\\:flex");
    expect(desktopOnly).not.toBeNull();
    // その中に SearchBar の input がある
    expect(desktopOnly?.querySelector('[data-testid="search-input"]')).not.toBeNull();
  });

  test("lg 未満の 2 段目に lg:hidden が付与され、SearchBar を含む", () => {
    const { container } = renderHeader();
    const mobileOnly = container.querySelector(".lg\\:hidden");
    expect(mobileOnly).not.toBeNull();
    expect(mobileOnly?.querySelector('[data-testid="search-input"]')).not.toBeNull();
  });

  test("ハンバーガーボタンが 44px 以上のタッチターゲット (py-2.5 + px-3) クラスを持つ", () => {
    renderHeader();
    const toggle = screen.getByTestId("sidebar-toggle");
    expect(toggle.className).toContain("py-2.5");
    expect(toggle.className).toContain("px-3");
  });
});
