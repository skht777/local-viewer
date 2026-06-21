// pdfjs getDocument に渡す PDF 読み込み共通オプション
// - disableAutoFetch: バックエンドが Range(206) に対応しているため、表示中ページに
//   必要なバイト範囲だけを取得する。PDF 全体の先読みを止めることで、大規模 PDF の
//   初回表示と先頭ページのみ必要なサムネイル生成を高速化する
// - cMapUrl / cMapPacked: 日本語など非埋め込み CID フォント PDF のテキスト描画に使う
//   文字マップ (vite.config.ts の copyPdfAssets が public/pdfjs/cmaps へ配置)
// - standardFontDataUrl: 標準14フォント (Helvetica 等) 非埋め込み時のフォールバック
// - lib/pdfjs（Worker 副作用を持つためテストでモックされる）とは別モジュールにして、
//   テストでも実値を参照できるようにする

export const PDF_LOAD_OPTIONS = {
  disableAutoFetch: true,
  cMapUrl: "/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "/pdfjs/standard_fonts/",
} as const;
