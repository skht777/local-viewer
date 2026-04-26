// SearchBar の履歴モデル遵守を検証する回帰テスト
// - viewer 起動（pdf/image）: replace 遷移 + viewerOrigin 設定 + mode/sort 継承
// - directory/archive: push 遷移、viewerOrigin 未設定
// - scope 未指定（TopPage 文脈）: viewerOrigin 未設定
// - viewerOrigin 設定判定は `scope` プロップ有無で行い、`isScopeActive` トグルには依存しない

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import type { SearchResult } from "../../src/types/api";

// useSearch を差し替え可能な結果配列に差し替える
const mockState = vi.hoisted(() => ({
  results: [] as SearchResult[],
}));

vi.mock("../../src/hooks/useSearch", async () => {
  const React = await vi.importActual<typeof import("react")>("react");
  return {
    useSearch: (_scope?: string) => {
      const [query, setQuery] = React.useState("");
      const [kind, setKind] = React.useState<string | null>(null);
      return {
        query,
        setQuery,
        debouncedQuery: query,
        kind,
        setKind,
        results: mockState.results,
        hasMore: false,
        isLoading: false,
        isError: false,
        isIndexing: false,
        refetch: () => {},
      };
    },
  };
});

// useNavigate をスパイに差し替え
const mockNavigate = vi.fn();
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>("react-router-dom");
  return { ...actual, useNavigate: () => mockNavigate };
});

import { SearchBar } from "../../src/components/SearchBar";
import { useViewerStore } from "../../src/stores/viewerStore";

function renderSearchBar(opts: { scope?: string; initialEntry?: string }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[opts.initialEntry ?? "/"]}>
        <SearchBar scope={opts.scope} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

async function openDropdown(user: ReturnType<typeof userEvent.setup>) {
  // debouncedQuery.length >= 2 でドロップダウン表示
  await user.type(screen.getByRole("textbox", { name: "検索" }), "te");
}

function makeResult(overrides: Partial<SearchResult> & Pick<SearchResult, "kind">): SearchResult {
  return {
    node_id: "result-1",
    parent_node_id: "parent-1",
    name: "item",
    relative_path: "mount/item",
    size_bytes: null,
    ...overrides,
  };
}

beforeEach(() => {
  mockNavigate.mockClear();
  mockState.results = [];
  useViewerStore.setState({ viewerOrigin: null });
});

describe("SearchBar 履歴モデル", () => {
  test("PDF 選択時は push 遷移し viewerOrigin と mode/sort を引き継ぐ", async () => {
    const user = userEvent.setup();
    mockState.results = [
      makeResult({ kind: "pdf", node_id: "pdf1", parent_node_id: "dir1", name: "doc.pdf" }),
    ];
    renderSearchBar({
      scope: "dir-scope",
      initialEntry: "/browse/dir-scope?mode=manga&sort=date-desc",
    });

    await openDropdown(user);
    await user.click(await screen.findByTestId("search-result-0"));

    expect(mockNavigate).toHaveBeenCalledTimes(1);
    const [[url, options]] = mockNavigate.mock.calls;
    expect(url).toMatch(/^\/browse\/dir1\?/);
    expect(url).toContain("pdf=pdf1");
    expect(url).toContain("mode=manga");
    expect(url).toContain("sort=date-desc");
    // push 遷移: ブラウザバックで呼び出し元に戻れるよう options 未指定
    expect(options).toBeUndefined();
    expect(useViewerStore.getState().viewerOrigin).toEqual({
      pathname: "/browse/dir-scope",
      search: "?mode=manga&sort=date-desc",
    });
  });

  test("画像選択時は replace 遷移し mode を引き継ぐ（sort 既定は省略）", async () => {
    const user = userEvent.setup();
    mockState.results = [
      makeResult({ kind: "image", node_id: "img1", parent_node_id: "dir1", name: "pic.jpg" }),
    ];
    renderSearchBar({
      scope: "dir-scope",
      initialEntry: "/browse/dir-scope?mode=manga",
    });

    await openDropdown(user);
    await user.click(await screen.findByTestId("search-result-0"));

    const [[url, options]] = mockNavigate.mock.calls;
    expect(url).toMatch(/^\/browse\/dir1\?/);
    expect(url).toContain("tab=images");
    expect(url).toContain("select=img1");
    expect(url).toContain("mode=manga");
    // 既定値は省略
    expect(url).not.toContain("sort=");
    expect(options).toEqual({ replace: true });
    expect(useViewerStore.getState().viewerOrigin).toEqual({
      pathname: "/browse/dir-scope",
      search: "?mode=manga",
    });
  });

  test("ディレクトリ選択時は通常 push 遷移で viewerOrigin を設定しない", async () => {
    const user = userEvent.setup();
    mockState.results = [
      makeResult({ kind: "directory", node_id: "dir-x", parent_node_id: null, name: "album" }),
    ];
    renderSearchBar({
      scope: "dir-scope",
      initialEntry: "/browse/dir-scope?mode=manga",
    });

    await openDropdown(user);
    await user.click(await screen.findByTestId("search-result-0"));

    expect(mockNavigate).toHaveBeenCalledTimes(1);
    const [[url, options]] = mockNavigate.mock.calls;
    expect(url).toBe("/browse/dir-x");
    // replace: true は渡さない（push 遷移）
    expect(options).toBeUndefined();
    expect(useViewerStore.getState().viewerOrigin).toBeNull();
  });

  test("scope 未指定（TopPage 文脈）では viewer 起動でも viewerOrigin を設定しない", async () => {
    const user = userEvent.setup();
    mockState.results = [
      makeResult({ kind: "pdf", node_id: "pdf1", parent_node_id: "dir1", name: "doc.pdf" }),
    ];
    renderSearchBar({
      initialEntry: "/",
    });

    await openDropdown(user);
    await user.click(await screen.findByTestId("search-result-0"));

    const [[, options]] = mockNavigate.mock.calls;
    // viewer 起動でも scope 無しなら replace せず push、origin も未設定
    expect(options).toBeUndefined();
    expect(useViewerStore.getState().viewerOrigin).toBeNull();
  });

  test("scope 有りかつトグル OFF 時も viewerOrigin は設定される", async () => {
    const user = userEvent.setup();
    mockState.results = [
      makeResult({ kind: "pdf", node_id: "pdf1", parent_node_id: "dir1", name: "doc.pdf" }),
    ];
    renderSearchBar({
      scope: "dir-scope",
      initialEntry: "/browse/dir-scope",
    });

    // スコープトグルを OFF にする（全体検索状態）
    await user.click(screen.getByTestId("scope-toggle"));

    await openDropdown(user);
    await user.click(await screen.findByTestId("search-result-0"));

    // isScopeActive=false でも scope プロップがある限り viewerOrigin は設定される
    expect(useViewerStore.getState().viewerOrigin).toEqual({
      pathname: "/browse/dir-scope",
      search: "",
    });
  });
});
