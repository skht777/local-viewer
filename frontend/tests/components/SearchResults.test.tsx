import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SearchResults } from "../../src/components/SearchResults";
import type { SearchResult } from "../../src/types/api";

const MOCK_RESULTS: SearchResult[] = [
  {
    node_id: "abc123",
    parent_node_id: "parent1",
    name: "photo.jpg",
    kind: "image",
    relative_path: "pictures/photo.jpg",
    size_bytes: 1000,
  },
  {
    node_id: "def456",
    parent_node_id: "parent2",
    name: "video.mp4",
    kind: "video",
    relative_path: "videos/video.mp4",
    size_bytes: 5000,
  },
];

describe("SearchResults", () => {
  test("結果がkindアイコン付きで表示される", () => {
    render(
      <SearchResults
        results={MOCK_RESULTS}
        hasMore={false}
        isLoading={false}
        isIndexing={false}
        activeIndex={-1}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    expect(screen.getByText("video.mp4")).toBeInTheDocument();
    expect(screen.getByText("pictures/photo.jpg")).toBeInTheDocument();
  });

  test("activeIndexの項目がハイライトされる", () => {
    render(
      <SearchResults
        results={MOCK_RESULTS}
        hasMore={false}
        isLoading={false}
        isIndexing={false}
        activeIndex={0}
        onSelect={() => {}}
      />,
    );
    const firstItem = screen.getByTestId("search-result-0");
    expect(firstItem.className).toContain("bg-blue-600/20");
  });

  test("クリックでonSelectが呼ばれる", async () => {
    const onSelect = vi.fn();
    render(
      <SearchResults
        results={MOCK_RESULTS}
        hasMore={false}
        isLoading={false}
        isIndexing={false}
        activeIndex={-1}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByText("photo.jpg"));
    expect(onSelect).toHaveBeenCalledWith(MOCK_RESULTS[0]);
  });

  test("0件時にメッセージが表示される", () => {
    render(
      <SearchResults
        results={[]}
        hasMore={false}
        isLoading={false}
        isIndexing={false}
        activeIndex={-1}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText("結果が見つかりません")).toBeInTheDocument();
  });

  test("ローディング中にメッセージが表示される", () => {
    render(
      <SearchResults
        results={[]}
        hasMore={false}
        isLoading={true}
        isIndexing={false}
        activeIndex={-1}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText("検索中...")).toBeInTheDocument();
  });

  test("インデックス構築中メッセージが表示される", () => {
    render(
      <SearchResults
        results={[]}
        hasMore={false}
        isLoading={false}
        isIndexing={true}
        activeIndex={-1}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText("インデックス構築中...")).toBeInTheDocument();
  });

  test("has_moreがtrueの時にさらに表示メッセージ", () => {
    render(
      <SearchResults
        results={MOCK_RESULTS}
        hasMore={true}
        isLoading={false}
        isIndexing={false}
        activeIndex={-1}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText("さらに結果があります...")).toBeInTheDocument();
  });
});
