// useBrowseTabAvailability の振る舞い検証
// - data 未取得時は空 set（disabled なし）
// - filesets 候補（directory/archive/pdf）が無ければ filesets を disabled
// - images が空なら images を disabled、videos が空なら videos を disabled
// - 全 3 タブ disabled になる場合は filesets を必ず残す

import { renderHook } from "@testing-library/react";
import { useBrowseTabAvailability } from "../../src/hooks/useBrowseTabAvailability";
import type { BrowseEntry, BrowseResponse } from "../../src/types/api";

function makeEntry(kind: BrowseEntry["kind"], id: string = kind): BrowseEntry {
  return {
    node_id: id,
    name: id,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

function makeData(entries: BrowseEntry[]): BrowseResponse {
  return {
    current_node_id: "n",
    current_name: "n",
    parent_node_id: null,
    ancestors: [],
    entries,
    next_cursor: null,
    total_count: null,
  };
}

describe("useBrowseTabAvailability", () => {
  test("data が undefined のときは空 set を返す", () => {
    const { result } = renderHook(() =>
      useBrowseTabAvailability({ data: undefined, images: [], videos: [] }),
    );
    expect(result.current.size).toBe(0);
  });

  test("ディレクトリ・画像・動画すべて存在するとき disabled は空", () => {
    const data = makeData([makeEntry("directory"), makeEntry("image"), makeEntry("video")]);
    const { result } = renderHook(() =>
      useBrowseTabAvailability({
        data,
        images: [makeEntry("image", "img")],
        videos: [makeEntry("video", "vid")],
      }),
    );
    expect(result.current.size).toBe(0);
  });

  test("filesets 候補（directory/archive/pdf）が無ければ filesets を disabled にする", () => {
    const data = makeData([makeEntry("image"), makeEntry("video")]);
    const { result } = renderHook(() =>
      useBrowseTabAvailability({
        data,
        images: [makeEntry("image", "img")],
        videos: [makeEntry("video", "vid")],
      }),
    );
    expect(result.current.has("filesets")).toBe(true);
    expect(result.current.has("images")).toBe(false);
    expect(result.current.has("videos")).toBe(false);
  });

  test("画像が空なら images を disabled", () => {
    const data = makeData([makeEntry("directory")]);
    const { result } = renderHook(() =>
      useBrowseTabAvailability({
        data,
        images: [],
        videos: [makeEntry("video", "vid")],
      }),
    );
    expect(result.current.has("images")).toBe(true);
    expect(result.current.has("filesets")).toBe(false);
  });

  test("動画が空なら videos を disabled", () => {
    const data = makeData([makeEntry("directory")]);
    const { result } = renderHook(() =>
      useBrowseTabAvailability({
        data,
        images: [makeEntry("image", "img")],
        videos: [],
      }),
    );
    expect(result.current.has("videos")).toBe(true);
  });

  test("すべて空なら filesets だけは残す（disabled が 3 → filesets を delete）", () => {
    const data = makeData([]);
    const { result } = renderHook(() => useBrowseTabAvailability({ data, images: [], videos: [] }));
    // filesets は残す
    expect(result.current.has("filesets")).toBe(false);
    // images / videos は disabled
    expect(result.current.has("images")).toBe(true);
    expect(result.current.has("videos")).toBe(true);
  });

  test("archive のみのときは filesets が有効、images/videos は disabled", () => {
    const data = makeData([makeEntry("archive")]);
    const { result } = renderHook(() => useBrowseTabAvailability({ data, images: [], videos: [] }));
    expect(result.current.has("filesets")).toBe(false);
    expect(result.current.has("images")).toBe(true);
    expect(result.current.has("videos")).toBe(true);
  });

  test("pdf も filesets 候補として扱われる", () => {
    const data = makeData([makeEntry("pdf")]);
    const { result } = renderHook(() => useBrowseTabAvailability({ data, images: [], videos: [] }));
    expect(result.current.has("filesets")).toBe(false);
  });
});
