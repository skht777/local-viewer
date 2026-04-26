import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { SearchBar } from "../../src/components/SearchBar";

// SearchBar は内部で useSearch + useNavigate を使うためプロバイダーが必要
function renderSearchBar(props?: { scope?: string }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <SearchBar scope={props?.scope} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("SearchBar", () => {
  test("入力欄が表示される", () => {
    renderSearchBar();
    expect(screen.getByPlaceholderText("全体を検索...")).toBeInTheDocument();
  });

  test("テキスト入力ができる", async () => {
    renderSearchBar();
    const input = screen.getByPlaceholderText("全体を検索...");
    await userEvent.type(input, "test query");
    expect(input).toHaveValue("test query");
  });

  test("kindフィルタボタンが表示される", () => {
    renderSearchBar();
    expect(screen.getByTestId("kind-filter-all")).toBeInTheDocument();
    expect(screen.getByTestId("kind-filter-directory")).toBeInTheDocument();
    expect(screen.getByTestId("kind-filter-video")).toBeInTheDocument();
  });

  test("aria-labelが設定されている", () => {
    renderSearchBar();
    expect(screen.getByLabelText("検索")).toBeInTheDocument();
  });

  describe("スコープトグル", () => {
    test("scopeプロップがない場合トグルは表示されない", () => {
      renderSearchBar();
      expect(screen.queryByTestId("scope-toggle")).not.toBeInTheDocument();
    });

    test("scopeプロップがある場合トグルが表示される", () => {
      renderSearchBar({ scope: "dir123" });
      expect(screen.getByTestId("scope-toggle")).toBeInTheDocument();
    });

    test("scopeプロップありの初期状態はフォルダ内検索が有効", () => {
      renderSearchBar({ scope: "dir123" });
      expect(screen.getByPlaceholderText("このフォルダ内を検索...")).toBeInTheDocument();
    });

    test("トグルクリックで全体検索に切り替わる", async () => {
      renderSearchBar({ scope: "dir123" });
      await userEvent.click(screen.getByTestId("scope-toggle"));
      expect(screen.getByPlaceholderText("全体を検索...")).toBeInTheDocument();
    });
  });

  describe("Enter キーで /search ページに遷移", () => {
    function LocationSpy({ onLoc }: { onLoc: (path: string, search: string) => void }) {
      const { useLocation } = require("react-router-dom") as typeof import("react-router-dom");
      const loc = useLocation();
      onLoc(loc.pathname, loc.search);
      return null;
    }

    test("候補非選択かつ q が 2 文字以上で /search?q=... に push される", async () => {
      let path = "";
      let search = "";
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      render(
        <QueryClientProvider client={queryClient}>
          <MemoryRouter initialEntries={["/"]}>
            <LocationSpy
              onLoc={(p, s) => {
                path = p;
                search = s;
              }}
            />
            <SearchBar />
          </MemoryRouter>
        </QueryClientProvider>,
      );
      const input = screen.getByPlaceholderText("全体を検索...");
      await userEvent.type(input, "hello");
      await userEvent.keyboard("{Enter}");
      expect(path).toBe("/search");
      expect(search).toContain("q=hello");
    });

    test("scope ON のとき /search?q=...&scope=... に push される", async () => {
      let path = "";
      let search = "";
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      render(
        <QueryClientProvider client={queryClient}>
          <MemoryRouter initialEntries={["/browse/dir-1"]}>
            <LocationSpy
              onLoc={(p, s) => {
                path = p;
                search = s;
              }}
            />
            <SearchBar scope="dir-1" />
          </MemoryRouter>
        </QueryClientProvider>,
      );
      const input = screen.getByPlaceholderText("このフォルダ内を検索...");
      await userEvent.type(input, "hello");
      await userEvent.keyboard("{Enter}");
      expect(path).toBe("/search");
      expect(search).toContain("q=hello");
      expect(search).toContain("scope=dir-1");
    });

    test("q が 1 文字以下では遷移しない", async () => {
      let path = "/";
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      render(
        <QueryClientProvider client={queryClient}>
          <MemoryRouter initialEntries={["/"]}>
            <LocationSpy
              onLoc={(p) => {
                path = p;
              }}
            />
            <SearchBar />
          </MemoryRouter>
        </QueryClientProvider>,
      );
      const input = screen.getByPlaceholderText("全体を検索...");
      await userEvent.type(input, "a");
      await userEvent.keyboard("{Enter}");
      expect(path).toBe("/");
    });
  });
});
