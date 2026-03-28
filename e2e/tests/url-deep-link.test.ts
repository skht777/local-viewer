// URL 直接遷移テスト (P1)
// UD-1: CG 直接 URL、UD-2: マンガ直接 URL、UD-3: PDF 直接 URL

import { test, expect } from "@playwright/test";

// API から node_id を動的取得するヘルパー
async function getNodeIds(request: import("@playwright/test").APIRequestContext) {
  const rootRes = await request.get("/api/browse");
  const root = await rootRes.json();

  // pictures マウントポイントの node_id
  const picturesMount = root.entries.find((e: { name: string }) => e.name === "pictures");
  if (!picturesMount) throw new Error("pictures マウントポイントが見つかりません");

  // docs マウントポイントの node_id
  const docsMount = root.entries.find((e: { name: string }) => e.name === "docs");
  if (!docsMount) throw new Error("docs マウントポイントが見つかりません");

  // docs 内の sample.pdf の node_id
  const docsRes = await request.get(`/api/browse/${docsMount.node_id}`);
  const docsData = await docsRes.json();
  const samplePdf = docsData.entries.find((e: { name: string }) => e.name === "sample.pdf");
  if (!samplePdf) throw new Error("sample.pdf が見つかりません");

  return {
    picturesNodeId: picturesMount.node_id as string,
    docsNodeId: docsMount.node_id as string,
    pdfNodeId: samplePdf.node_id as string,
  };
}

test.describe("URL 直接遷移", () => {
  test("UD-1: 直接 URL で CG モード 3ページ目が開く", async ({ page, request }) => {
    const { picturesNodeId } = await getNodeIds(request);

    await page.goto(`/browse/${picturesNodeId}?tab=images&index=2&mode=cg`);

    await expect(page.getByTestId("cg-viewer")).toBeVisible();
    const counter = page.getByTestId("page-counter");
    await expect(counter).toContainText(/3\s*\/\s*\d+/);
  });

  test("UD-2: 直接 URL でマンガモードが開く", async ({ page, request }) => {
    const { picturesNodeId } = await getNodeIds(request);

    await page.goto(`/browse/${picturesNodeId}?tab=images&index=0&mode=manga`);

    await expect(page.getByTestId("manga-viewer")).toBeVisible();
  });

  test("UD-3: PDF 直接 URL で指定ページが開く", async ({ page, request }) => {
    const { docsNodeId, pdfNodeId } = await getNodeIds(request);

    await page.goto(`/browse/${docsNodeId}?pdf=${pdfNodeId}&page=2&mode=cg`);

    await expect(page.getByTestId("pdf-cg-viewer")).toBeVisible();
    const counter = page.getByTestId("page-counter");
    await expect(counter).toContainText(/2\s*\/\s*2/);
  });
});
