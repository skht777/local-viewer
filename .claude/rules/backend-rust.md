---
paths:
  - "rust-backend/**/*.rs"
---

# Rust バックエンド固有規約

## アーキテクチャ
```
routers → services → 外部クレート/std
```
- Python 版と同じレイヤード依存方向を維持
- routers は services を呼ぶ。直接ファイルシステムや DB にアクセスしない
- services は他の services に依存してよいが、routers に依存しない
- path_security は全てのファイルアクセスの前に必ず経由する

## axum パターン

### ルーティング
- 1 リソース 1 ファイル: `mounts.rs`, `browse.rs`, `file.rs`, `thumbnail.rs`, `search.rs`
- 全ルート `/api/` プレフィックス
- node_id パスパラメータを使用、生ファイルパスは公開しない
- axum 0.8 のパス構文: `/{node_id}` (`/:id` ではない)

### 状態管理 (DI)
- `AppState` は基本的に不変。`Arc<T>` でサービスを共有する
- `axum::extract::State<Arc<AppState>>` でハンドラに注入
- 共有可変状態は最小化する。不変 `Arc<T>` を優先
- 可変が必要な場合: `await` をまたがないなら `std::sync::Mutex`、またぐなら `tokio::sync::Mutex`
- `Arc<RwLock<T>>` は読み取りが支配的で `await` をまたがない場合のみ

### レスポンス
- 共通エラーモデル: `{"error": str, "code": str, "detail"?: any}`
- ファイルレスポンス: `tower-http::services::ServeFile` で配信。ETag と Cache-Control は明示的にヘッダを付与する (`ServeFile` は自動で付けない)
- JSON レスポンス: `axum::Json<T>` (T: Serialize)
- エラーは `IntoResponse` トレイトで統一変換

### 非同期
- ルートハンドラは `async fn`
- イベントループを絶対にブロックしない
- ブロッキング I/O: `tokio::task::spawn_blocking` 内で `std::fs` を使用
- `std::fs::read_dir` はブロッキング I/O。必ず `spawn_blocking` 内、または同期サービス内部で使う

## エラーハンドリング
- サービス層: `thiserror` で型付きエラー定義
- ルーター層: `IntoResponse` で HTTP レスポンスに変換
- `anyhow` はテストと main.rs のみ
- `?` で伝播し `.context("説明")` を付ける。`unwrap()` / `expect()` はテスト以外で禁止

## メモリ管理
- `Arc` で共有所有権、`&` で借用を基本とする
- 大きなデータ (アーカイブ bytes 等) は `Bytes` 型を使用 (参照カウント、ゼロコピー)
- キャッシュは `moka` クレート (W-TinyLFU、バイトベース制限、非同期対応)

## CPU バウンド処理とスレッドモデル
- `rayon`: CPU バウンドな純粋並列処理 (画像リサイズ、アーカイブ展開、並列 stat)。`tokio::sync::oneshot` チャネルで結果を受け取る
- `spawn_blocking`: 同期ライブラリ呼び出し (rusqlite、`std::fs`、外部 CLI)。最大 500 スレッド、I/O 待ちに適切
- SQLite 操作: `spawn_blocking` 内で同期 rusqlite を使用 (WAL モードで並行読み取り可)
- バックグラウンドタスク: `tokio::spawn` で fire-and-forget (`JoinSet` で必要に応じて追跡)
- 並行度制限: `tokio::sync::Semaphore` (サムネイルプリウォーム等)

## Lint / 品質
- lint 設定は `Cargo.toml` の `[lints]` セクションで管理 (ソースコード内 `#![warn]` は使わない)
- `clippy::pedantic` 有効 + false positive の多い lint を個別に `allow`
- `clippy::restriction` から `dbg_macro`, `todo`, `print_stdout`, `unwrap_used` 等を cherry-pick
- `unsafe_code = "forbid"` (unsafe ブロック完全禁止)
- `cargo-deny` で脆弱性・ライセンスチェック (`deny.toml`)

## ログ
- `tracing` クレートで構造化ログ
- `#[tracing::instrument]` でハンドラのスパン自動生成
- ログレベル: ERROR (障害), WARN (回復可能), INFO (起動/シャットダウン), DEBUG (リクエスト詳細), TRACE (内部詳細)
- `println!` / `eprintln!` は禁止 (`print_stdout` / `print_stderr` lint で検出)

## 設定
- 環境変数ベース (Python 版 config.py と同一変数名・デフォルト値)
- `clap` で CLI 引数 (`--port`, `--bind`)
- 起動時にバリデーション、不正値は panic ではなくエラーメッセージ付き終了

## 依存クレート補足
- `rusqlite` の `bundled-full`: FTS5 trigram トークナイザが必要なため `bundled` では不足。`bundled-full` で FTS5 + その他拡張を有効化
- `md-5` (Cargo パッケージ名) / `md5` (クレートパス): ETag 生成専用。セキュリティ用途ではない
- 外部ランタイム依存: `unrar-free`, `p7zip-full`, `ffmpeg`, `poppler-utils` (Docker イメージに同梱)

## Python 互換性 (移行期間中)
- HMAC node_id: `"{root}::{relative}"` フォーマット、SHA256 先頭 16 hex 文字
- カーソル: `json(sort_keys=True, separators=(",",":"))` → base64 URL-safe。`BTreeMap` でキーソート保証
- 自然順ソート: `[0-9]+` で分割 (Unicode `\d` ではない)、小文字化
- DirIndex sort_key: 10 桁ゼロ埋め + `\x00` 区切り
- エラーコード: `FORBIDDEN_PATH`, `NOT_FOUND`, `ARCHIVE_SECURITY_ERROR` 等を Python 版と一致させる
