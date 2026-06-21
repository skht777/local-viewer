// pdfjs getDocument に渡す PDF 読み込み共通オプション
// - cMapUrl / cMapPacked: 日本語など非埋め込み CID フォント PDF のテキスト描画に使う
//   文字マップ (vite.config.ts の copyPdfAssets が public/pdfjs/cmaps へ配置)
// - standardFontDataUrl: 標準14フォント (Helvetica 等) 非埋め込み時のフォールバック
// - lib/pdfjs（Worker 副作用を持つためテストでモックされる）とは別モジュールにして、
//   テストでも実値を参照できるようにする
//
// 注: disableAutoFetch は撤回済み。FileCard のサムネイル(usePdfThumbnail)と
// ビューワー(usePdfDocument)が同一 PDF を二重ロードするため、Range ベースの遅延取得は
// コールドキャッシュ時に並行干渉して初回描画が空になることがある（再読み込みで復帰）。
// 非線形化 PDF では pdfjs が全オブジェクト索引で結局全取得するため実利も薄く、
// 既定（全取得）に戻して安定性を優先する。
export const PDF_LOAD_OPTIONS = {
  cMapUrl: "/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "/pdfjs/standard_fonts/",
} as const;
