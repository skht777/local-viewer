// FileBrowser ソート（サーバー側ソート済み配列の表示順維持）
// - name-asc では filesets タブ内で archive/PDF 優先のグループ順を維持
// - date 系は null エントリの末尾保持を含む

import { screen } from "@testing-library/react";
import { FileBrowser } from "../../src/components/FileBrowser";
import type { BrowseEntry } from "../../src/types/api";
import {
  makeDirectoryEntry,
  makeImageEntry,
  makePdfEntry,
  renderFileBrowser,
} from "./__helpers__/fileBrowserTestHelpers";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
  GlobalWorkerOptions: { workerSrc: "" },
}));

describe("FileBrowser ソート", () => {
  test("sort=date-descでサーバーソート済みの降順が維持される", () => {
    // サーバーサイドソート済み: 日付降順
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "new.jpg", modified_at: 3000 }),
      makeImageEntry({ node_id: "i3", name: "mid.jpg", modified_at: 2000 }),
      makeImageEntry({ node_id: "i1", name: "old.jpg", modified_at: 1000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="date-desc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    const names = cards.map((c) => c.textContent);
    expect(names[0]).toContain("new.jpg");
    expect(names[1]).toContain("mid.jpg");
    expect(names[2]).toContain("old.jpg");
  });

  test("sort=date-ascでサーバーソート済みの昇順が維持される", () => {
    // サーバーサイドソート済み: 日付昇順
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "old.jpg", modified_at: 1000 }),
      makeImageEntry({ node_id: "i1", name: "new.jpg", modified_at: 3000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="date-asc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    expect(cards[0].textContent).toContain("old.jpg");
    expect(cards[1].textContent).toContain("new.jpg");
  });

  test("sort=name-descでサーバーソート済みの名前降順が維持される", () => {
    // サーバーサイドソート済み: 名前降順
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "beta.jpg", modified_at: 2000 }),
      makeImageEntry({ node_id: "i1", name: "alpha.jpg", modified_at: 1000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="name-desc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    expect(cards[0].textContent).toContain("beta.jpg");
    expect(cards[1].textContent).toContain("alpha.jpg");
  });

  test("sort=name-ascでディレクトリ優先の名前順が維持される", () => {
    const entries: BrowseEntry[] = [
      makePdfEntry({ node_id: "f1", name: "aaa.pdf", size_bytes: 100, modified_at: 1000 }),
      makeDirectoryEntry({ node_id: "d1", name: "bbb_dir", child_count: 5, modified_at: 2000 }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    const names = cards.map((c) => c.textContent);
    // filesets タブ: archive/PDF 優先 → directory 後（既存動作維持）
    const pdfIdx = names.findIndex((n) => n?.includes("aaa.pdf"));
    const dirIdx = names.findIndex((n) => n?.includes("bbb_dir"));
    expect(pdfIdx).toBeLessThan(dirIdx);
  });

  test("sort=date-descでサーバーソート済みのnullエントリが最後に維持される", () => {
    // サーバーサイドソート済み: 日付降順、null は末尾
    const entries: BrowseEntry[] = [
      makeImageEntry({ node_id: "i2", name: "has-date.jpg", modified_at: 1000 }),
      makeImageEntry({ node_id: "i1", name: "no-date.jpg", modified_at: null }),
    ];
    renderFileBrowser(
      <FileBrowser
        entries={entries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="date-desc"
      />,
    );
    const cards = screen.getAllByTestId(/^file-card-/);
    expect(cards[0].textContent).toContain("has-date.jpg");
    expect(cards[1].textContent).toContain("no-date.jpg");
  });
});
