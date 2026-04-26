// useBrowseTabAutoSwitch の振る舞い検証
// - 現在タブにコンテンツがあれば setTab を呼ばない
// - 現在タブが空なら filesets > images > videos の優先順で自動切替
// - すべて空なら setTab を呼ばない（現在タブに留まる）
// - data 未取得 / loading 中は setTab を呼ばない

import { renderHook } from "@testing-library/react";
import { useBrowseTabAutoSwitch } from "../../src/hooks/useBrowseTabAutoSwitch";
import type { ViewerTab } from "../../src/hooks/useViewerParams";
import type { BrowseEntry, BrowseResponse } from "../../src/types/api";

function makeEntry(kind: BrowseEntry["kind"]): BrowseEntry {
  return {
    node_id: kind,
    name: kind,
    kind,
    size_bytes: null,
    mime_type: null,
    child_count: null,
    modified_at: null,
    preview_node_ids: null,
  };
}

function makeData(kinds: BrowseEntry["kind"][]): BrowseResponse {
  return {
    current_node_id: "n",
    current_name: "n",
    parent_node_id: null,
    ancestors: [],
    entries: kinds.map(makeEntry),
    next_cursor: null,
    total_count: null,
  };
}

interface Run {
  data?: BrowseResponse;
  isLoading?: boolean;
  currentTab: ViewerTab;
}

function run({ data, isLoading = false, currentTab }: Run) {
  const setTab = vi.fn();
  renderHook(() => useBrowseTabAutoSwitch({ data, isLoading, currentTab, setTab }));
  return setTab;
}

describe("useBrowseTabAutoSwitch", () => {
  test("data が undefined のときは setTab を呼ばない", () => {
    const setTab = run({ currentTab: "filesets" });
    expect(setTab).not.toHaveBeenCalled();
  });

  test("isLoading のときは setTab を呼ばない", () => {
    const setTab = run({
      data: makeData(["image"]),
      isLoading: true,
      currentTab: "filesets",
    });
    expect(setTab).not.toHaveBeenCalled();
  });

  test("filesets タブで directory があるとき切替しない", () => {
    const setTab = run({ data: makeData(["directory"]), currentTab: "filesets" });
    expect(setTab).not.toHaveBeenCalled();
  });

  test("filesets タブで directory も pdf も archive も無いとき images タブに切替", () => {
    const setTab = run({ data: makeData(["image"]), currentTab: "filesets" });
    expect(setTab).toHaveBeenCalledWith("images");
  });

  test("filesets タブで images が無く videos があれば videos に切替", () => {
    const setTab = run({ data: makeData(["video"]), currentTab: "filesets" });
    expect(setTab).toHaveBeenCalledWith("videos");
  });

  test("images タブで images があれば切替しない", () => {
    const setTab = run({ data: makeData(["image"]), currentTab: "images" });
    expect(setTab).not.toHaveBeenCalled();
  });

  test("images タブで images が無く directory があれば filesets に切替", () => {
    const setTab = run({ data: makeData(["directory"]), currentTab: "images" });
    expect(setTab).toHaveBeenCalledWith("filesets");
  });

  test("videos タブで videos が無く images だけあれば images に切替（filesets 優先しない）", () => {
    const setTab = run({ data: makeData(["image"]), currentTab: "videos" });
    expect(setTab).toHaveBeenCalledWith("images");
  });

  test("videos タブで directory も image も無いと自動切替しない", () => {
    const setTab = run({ data: makeData(["video"]), currentTab: "videos" });
    expect(setTab).not.toHaveBeenCalled();
  });

  test("entries が空ならどのタブでも setTab を呼ばない", () => {
    const setTab = run({ data: makeData([]), currentTab: "filesets" });
    expect(setTab).not.toHaveBeenCalled();
  });

  test("archive / pdf も filesets 候補として扱う", () => {
    const archiveTab = run({ data: makeData(["archive"]), currentTab: "images" });
    expect(archiveTab).toHaveBeenCalledWith("filesets");
    const pdfTab = run({ data: makeData(["pdf"]), currentTab: "videos" });
    expect(pdfTab).toHaveBeenCalledWith("filesets");
  });
});
