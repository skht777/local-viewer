import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FileCard } from "../../src/components/FileCard";
import type { BrowseEntry } from "../../src/types/api";

vi.mock("../../src/lib/pdfjs", () => ({
  getDocument: vi.fn(),
  GlobalWorkerOptions: { workerSrc: "" },
}));

const imageEntry: BrowseEntry = {
  node_id: "img001",
  name: "photo.jpg",
  kind: "image",
  size_bytes: 2048,
  mime_type: "image/jpeg",
  child_count: null,
  modified_at: null,
  preview_node_ids: null,
};

const dirEntry: BrowseEntry = {
  node_id: "dir001",
  name: "folder",
  kind: "directory",
  size_bytes: null,
  mime_type: null,
  child_count: 5,
  modified_at: null,
  preview_node_ids: null,
};

// 既存テスト用ヘルパー（旧 onClick → 新 onSelect/onDoubleClick）
const noop = () => {};

describe("FileCard", () => {
  test("画像エントリでバッチサムネイルのimgタグが表示される", () => {
    render(
      <FileCard
        entry={imageEntry}
        onSelect={noop}
        onDoubleClick={noop}
        batchThumbnailUrl="blob:mock-thumb"
      />,
    );
    const img = screen.getByRole("img");
    expect(img).toHaveAttribute("src", "blob:mock-thumb");
  });

  test("ディレクトリエントリでアイコンが表示される", () => {
    render(<FileCard entry={dirEntry} onSelect={noop} onDoubleClick={noop} />);
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText("folder")).toBeInTheDocument();
  });

  test("ファイルサイズが表示される", () => {
    render(<FileCard entry={imageEntry} onSelect={noop} onDoubleClick={noop} />);
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
  });

  test("isSelected=true で aria-current='true' が設定される", () => {
    render(<FileCard entry={imageEntry} onSelect={noop} onDoubleClick={noop} isSelected />);
    const card = screen.getByTestId("file-card-img001");
    expect(card).toHaveAttribute("aria-current", "true");
  });

  test("isSelected=false で aria-current が設定されない", () => {
    render(<FileCard entry={imageEntry} onSelect={noop} onDoubleClick={noop} />);
    const card = screen.getByTestId("file-card-img001");
    expect(card).not.toHaveAttribute("aria-current");
  });
});

// --- C2: 新しい操作モデルのテスト ---

const archiveEntry: BrowseEntry = {
  node_id: "arc001",
  name: "photos.zip",
  kind: "archive",
  size_bytes: 10240,
  mime_type: "application/zip",
  child_count: null,
  modified_at: null,
  preview_node_ids: null,
};

const pdfEntry: BrowseEntry = {
  node_id: "pdf001",
  name: "doc.pdf",
  kind: "pdf",
  size_bytes: 4096,
  mime_type: "application/pdf",
  child_count: null,
  modified_at: null,
  preview_node_ids: null,
};

describe("FileCard 選択・ダブルクリック・オーバーレイ", () => {
  test("シングルクリックでonSelectが呼ばれる", async () => {
    const onSelect = vi.fn();
    render(
      <FileCard entry={dirEntry} onSelect={onSelect} onDoubleClick={() => {}} />,
    );
    await userEvent.click(screen.getByTestId("file-card-dir001"));
    expect(onSelect).toHaveBeenCalledWith(dirEntry);
  });

  test("ダブルクリックでonDoubleClickが呼ばれる", async () => {
    const onDoubleClick = vi.fn();
    render(
      <FileCard entry={dirEntry} onSelect={() => {}} onDoubleClick={onDoubleClick} />,
    );
    await userEvent.dblClick(screen.getByTestId("file-card-dir001"));
    expect(onDoubleClick).toHaveBeenCalledWith(dirEntry);
  });

  test("isSelected=trueでアクションオーバーレイが表示される", () => {
    render(
      <FileCard
        entry={dirEntry}
        onSelect={() => {}}
        onDoubleClick={() => {}}
        onOpen={() => {}}
        isSelected
      />,
    );
    expect(screen.getByTestId("action-overlay-dir001")).toBeInTheDocument();
  });

  test("ディレクトリ選択時にオーバーレイに開くと進入ボタンが表示される", () => {
    render(
      <FileCard
        entry={dirEntry}
        onSelect={() => {}}
        onDoubleClick={() => {}}
        onOpen={() => {}}
        onEnter={() => {}}
        isSelected
      />,
    );
    expect(screen.getByTestId("action-open-dir001")).toBeInTheDocument();
    expect(screen.getByTestId("action-enter-dir001")).toBeInTheDocument();
  });

  test("画像選択時にオーバーレイに開くボタンのみ表示される", () => {
    render(
      <FileCard
        entry={imageEntry}
        onSelect={() => {}}
        onDoubleClick={() => {}}
        onOpen={() => {}}
        isSelected
      />,
    );
    expect(screen.getByTestId("action-open-img001")).toBeInTheDocument();
    expect(screen.queryByTestId("action-enter-img001")).not.toBeInTheDocument();
  });

  test("PDF選択時にオーバーレイに開くボタンのみ表示される", () => {
    render(
      <FileCard
        entry={pdfEntry}
        onSelect={() => {}}
        onDoubleClick={() => {}}
        onOpen={() => {}}
        isSelected
      />,
    );
    expect(screen.getByTestId("action-open-pdf001")).toBeInTheDocument();
    expect(screen.queryByTestId("action-enter-pdf001")).not.toBeInTheDocument();
  });

  test("オーバーレイの開くボタンクリックでonOpenが呼ばれる", async () => {
    const onOpen = vi.fn();
    render(
      <FileCard
        entry={dirEntry}
        onSelect={() => {}}
        onDoubleClick={() => {}}
        onOpen={onOpen}
        isSelected
      />,
    );
    await userEvent.click(screen.getByTestId("action-open-dir001"));
    expect(onOpen).toHaveBeenCalledWith(dirEntry);
  });

  test("オーバーレイの進入ボタンクリックでonEnterが呼ばれる", async () => {
    const onEnter = vi.fn();
    render(
      <FileCard
        entry={dirEntry}
        onSelect={() => {}}
        onDoubleClick={() => {}}
        onOpen={() => {}}
        onEnter={onEnter}
        isSelected
      />,
    );
    await userEvent.click(screen.getByTestId("action-enter-dir001"));
    expect(onEnter).toHaveBeenCalledWith(dirEntry);
  });

  test("isSelected=falseでオーバーレイが表示されない", () => {
    render(
      <FileCard
        entry={dirEntry}
        onSelect={() => {}}
        onDoubleClick={() => {}}
        onOpen={() => {}}
        onEnter={() => {}}
      />,
    );
    expect(screen.queryByTestId("action-overlay-dir001")).not.toBeInTheDocument();
  });

  test("Enterキーでダブルクリックと同じ動作になる", async () => {
    const onDoubleClick = vi.fn();
    render(
      <FileCard entry={dirEntry} onSelect={() => {}} onDoubleClick={onDoubleClick} />,
    );
    screen.getByTestId("file-card-dir001").focus();
    await userEvent.keyboard("{Enter}");
    expect(onDoubleClick).toHaveBeenCalledWith(dirEntry);
  });

  test("SpaceキーでonOpenが呼ばれる（onOpen指定時）", async () => {
    const onOpen = vi.fn();
    render(
      <FileCard entry={dirEntry} onSelect={() => {}} onDoubleClick={() => {}} onOpen={onOpen} />,
    );
    screen.getByTestId("file-card-dir001").focus();
    await userEvent.keyboard(" ");
    expect(onOpen).toHaveBeenCalledWith(dirEntry);
  });

  test("SpaceキーでonSelectにフォールバック（onOpen未指定時）", async () => {
    const onSelect = vi.fn();
    render(
      <FileCard entry={dirEntry} onSelect={onSelect} onDoubleClick={() => {}} />,
    );
    screen.getByTestId("file-card-dir001").focus();
    await userEvent.keyboard(" ");
    expect(onSelect).toHaveBeenCalledWith(dirEntry);
  });
});

// --- A4: プレビューサムネイル表示テスト ---

const dirWithPreview: BrowseEntry = {
  ...dirEntry,
  node_id: "dir_prev",
  preview_node_ids: ["prev1", "prev2"],
};

describe("FileCard プレビュー表示", () => {
  test("ディレクトリでpreview_node_idsがある場合プレビューが表示される", () => {
    const thumbs = new Map([
      ["prev1", "blob:prev1"],
      ["prev2", "blob:prev2"],
    ]);
    const { container } = render(
      <FileCard
        entry={dirWithPreview}
        onSelect={noop}
        onDoubleClick={noop}
        batchThumbnails={thumbs}
      />,
    );
    // PreviewGrid が表示され、バッチサムネイル画像が存在する
    const imgs = container.querySelectorAll("img");
    expect(imgs.length).toBeGreaterThanOrEqual(1);
    expect(imgs[0]).toHaveAttribute("src", "blob:prev1");
  });

  test("アーカイブでサムネイルプレビューが表示される", () => {
    const { container } = render(
      <FileCard
        entry={archiveEntry}
        onSelect={noop}
        onDoubleClick={noop}
        batchThumbnailUrl="blob:arc-thumb"
      />,
    );
    const imgs = container.querySelectorAll("img");
    expect(imgs).toHaveLength(1);
    expect(imgs[0]).toHaveAttribute("src", "blob:arc-thumb");
  });

  test("preview_node_idsがnullの場合絵文字が表示される", () => {
    render(<FileCard entry={dirEntry} onSelect={noop} onDoubleClick={noop} />);
    // img タグがなく、テキストコンテンツにフォルダ名が表示される
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText("folder")).toBeInTheDocument();
  });

  test("画像エントリはバッチサムネイルで表示される", () => {
    render(
      <FileCard
        entry={imageEntry}
        onSelect={noop}
        onDoubleClick={noop}
        batchThumbnailUrl="blob:img-thumb"
      />,
    );
    const img = screen.getByRole("img");
    expect(img).toHaveAttribute("src", "blob:img-thumb");
  });
});
