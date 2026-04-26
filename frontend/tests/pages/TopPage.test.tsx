// TopPage ローディング/エラー UI テスト

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import TopPage from "../../src/pages/TopPage";
import { renderWithProviders } from "../helpers/renderWithProviders";

function renderTopPage() {
  return renderWithProviders(
    <Routes>
      <Route path="/" element={<TopPage />} />
    </Routes>,
    { initialEntries: ["/"] },
  );
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("TopPage", () => {
  test("ローディング中にスケルトンが表示される", () => {
    // fetch を遅延させてローディング状態を維持
    globalThis.fetch = vi.fn(() => new Promise(() => {})) as typeof fetch;

    renderTopPage();
    // animate-pulse クラスを持つスケルトン要素が存在する
    const skeletons = document.querySelectorAll(".animate-pulse");
    expect(skeletons.length).toBeGreaterThan(0);
  });

  test("エラー時にリトライボタンが表示される", async () => {
    globalThis.fetch = vi.fn(() => Promise.reject(new Error("Network error"))) as typeof fetch;

    renderTopPage();

    await waitFor(() => {
      expect(screen.getByText(/エラーが発生しました/)).toBeTruthy();
    });
    expect(screen.getByTestId("retry-button")).toBeTruthy();
  });

  test("リトライボタンクリックで再取得される", async () => {
    let callCount = 0;
    globalThis.fetch = vi.fn(() => {
      callCount++;
      if (callCount <= 1) {
        return Promise.reject(new Error("Network error"));
      }
      return Promise.resolve(new Response(JSON.stringify({ mounts: [] })));
    }) as typeof fetch;

    renderTopPage();

    await waitFor(() => {
      expect(screen.getByTestId("retry-button")).toBeTruthy();
    });

    await userEvent.click(screen.getByTestId("retry-button"));

    await waitFor(() => {
      expect(screen.queryByTestId("retry-button")).toBeNull();
    });
  });
});
