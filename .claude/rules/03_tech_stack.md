# 技術スタック

## バックエンド (Rust)
- Rust (stable, rust-toolchain.toml で固定)
- axum + tokio (HTTP フレームワーク + 非同期ランタイム)
- tower + tower-http (ミドルウェア: CORS, GZip, 静的ファイル)
- serde + serde_json (JSON シリアライズ)
- rusqlite (`bundled` feature, FTS5 trigram 対応)
- hmac + sha2 (HMAC-SHA256 node_id 生成)
- image + fast_image_resize (サムネイル生成)
- zip クレート (ZIP), unrar-free CLI (RAR), p7zip CLI (7z, subprocess)
- moka (W-TinyLFU キャッシュ, sync)
- notify (ファイル監視)
- rayon (並列ディレクトリ走査)
- clap (CLI 引数パース)
- tracing + tracing-subscriber (構造化ログ)

## フロントエンド
- React 19 + TypeScript
- Vite (ビルド + 開発サーバー)
- Tailwind CSS v4 (@tailwindcss/vite プラグイン)
- TanStack Query (サーバー状態管理)
- zustand (UI状態管理)
- react-hotkeys-hook (キーボードショートカット)
- @tanstack/react-virtual (仮想スクロール)
- pdfjs-dist (PDF描画)
- react-router-dom (ルーティング)
- vite-plugin-pwa (PWA キャッシュ: サムネイル CacheFirst, API NetworkFirst)

## 開発ツール
- clippy (Rust lint)
- rustfmt (Rust format)
- cargo-deny (脆弱性・ライセンスチェック)
- oxlint (TypeScript lint)
- oxfmt (TypeScript format)
- Vitest + Testing Library (フロントエンドテスト)
- cargo test + rstest (Rust バックエンドテスト)
- Husky + lint-staged (pre-commit)

## コンテナ
- Docker (マルチステージビルド)
- docker-compose

## 禁止事項
- 上記以外のUIフレームワーク（Angular, Vue等）を提案しない
- ESLint, Prettier, Biome を提案しない（oxlint + oxfmt を使用）
- react-pdf を提案しない（pdfjs-dist を直接使用）
- CSS Modules, styled-components, Emotion を提案しない（Tailwind のみ）
- Redux, MobX, Recoil を提案しない（zustand + TanStack Query）
- actix-web, warp, rocket を提案しない（axum を使用）
- Pillow 相当のクレートを提案しない（image + fast_image_resize を使用）
- diesel, sea-orm を提案しない（rusqlite を直接使用）
- `unsafe` ブロック完全禁止 (`unsafe_code = "forbid"`)
