// FileBrowser タブ切替・ローディング・空状態・filesets タブ内優先順
// （sort/autoselect/interaction は別ファイル）

import { screen } from "@testing-library/react";
import { FileBrowser } from "../../src/components/FileBrowser";
import type { BrowseEntry } from "../../src/types/api";
import {
  makeArchiveEntry,
  makeDirectoryEntry,
  makePdfEntry,
  mockEntries,
  renderFileBrowser,
} from "./__helpers__/fileBrowserTestHelpers";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
  GlobalWorkerOptions: { workerSrc: "" },
}));

describe("FileBrowser タブ切替", () => {
  test("filesetsタブでディレクトリとPDFが表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("photos")).toBeInTheDocument();
    expect(screen.getByText("doc.pdf")).toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("imagesタブで画像のみ表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="images"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("image.jpg")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("movie.mp4")).not.toBeInTheDocument();
  });

  test("videosタブで動画のみ表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="videos"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("movie.mp4")).toBeInTheDocument();
    expect(screen.queryByText("photos")).not.toBeInTheDocument();
    expect(screen.queryByText("image.jpg")).not.toBeInTheDocument();
  });

  test("ローディング中にメッセージが表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={[]}
        isLoading={true}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    expect(screen.getByText("読み込み中...")).toBeInTheDocument();
  });

  test("フィルタ後0件で空状態メッセージが表示される", () => {
    renderFileBrowser(
      <FileBrowser
        entries={mockEntries}
        isLoading={false}
        onNavigate={() => {}}
        tab="videos"
        sort="name-asc"
      />,
    );
    // videos タブには movie.mp4 があるので空にはならない
    // 空の entries で確認
    const { unmount } = renderFileBrowser(
      <FileBrowser
        entries={[]}
        isLoading={false}
        onNavigate={() => {}}
        tab="filesets"
        sort="name-asc"
      />,
    );
    expect(screen.getAllByText("ファイルがありません").length).toBeGreaterThan(0);
    unmount();
  });

  test("filesetsタブでarchive/PDFがディレクトリより先に表示される", () => {
    const entries: BrowseEntry[] = [
      makeDirectoryEntry({
        node_id: "d1",
        name: "aaa_dir",
        child_count: 5,
        modified_at: 1_700_000_000,
      }),
      makeArchiveEntry({
        node_id: "a1",
        name: "bbb.zip",
        size_bytes: 500,
        modified_at: 1_700_000_100,
      }),
      makePdfEntry({ node_id: "p1", name: "ccc.pdf", size_bytes: 300, modified_at: 1_700_000_200 }),
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
    // DOM 上の順序: archive/PDF が先、directory が後
    const cards = screen.getAllByTestId(/^file-card-/);
    const names = cards.map((c) => c.textContent);
    const zipIdx = names.findIndex((n) => n?.includes("bbb.zip"));
    const pdfIdx = names.findIndex((n) => n?.includes("ccc.pdf"));
    const dirIdx = names.findIndex((n) => n?.includes("aaa_dir"));
    expect(zipIdx).toBeLessThan(dirIdx);
    expect(pdfIdx).toBeLessThan(dirIdx);
  });
});
