# セキュリティ規約

## 基本原則
- セキュリティリスクのあるコードは絶対に生成しない
- やむを得ない場合は必ず警告を表示し、安全な代替案を提示する

## パストラバーサル防止
- 全ファイルアクセスは `path_security.rs` を経由する
- `std::fs::canonicalize()` 後に登録されたマウントポイント (`root_dirs`) のいずれか配下であることを検証
- `find_root_for()` で対象パスが属するルートディレクトリを特定
- symlink はデフォルトで追跡しない（設定で許可可能）
- node_id 不透明ID方式 — クライアントに実パスを公開しない（ルートパスを含めて生成し、複数マウントポイント間の衝突を回避）
- `MOUNT_BASE_DIR` 環境変数でコンテナ内のマウント親ディレクトリを定義。`manage_mounts.sh` がホスト側パスのバリデーションを担当
- `validate_slug()` でバックエンド側も slug の安全性を検証（`../`, `/`, NUL バイト等を拒否）
- **永続層・キャッシュ復元パスの `join` 前検証必須**: 永続 SQLite / キャッシュから読み出した相対パスを `root.join()` する経路では、`Path::components()` を走査し `Component::Normal` 以外（`ParentDir`, `CurDir`, `RootDir`, `Prefix`）を 1 つでも含む場合は reject する lexical validation を実装する。`register_resolved` / `find_root_for` の root ガードは文字列 `starts_with` 判定で、canonicalized 入力を前提とした最終防壁であり、DB 復元経路の代替にはならない

## アーカイブ安全性
- エントリ名検証: `../`, 絶対パス, NULバイト を拒否
- zip bomb対策: 展開後サイズ上限 (1GB), 圧縮率上限 (100:1), 1エントリ最大サイズ
- 許可拡張子ホワイトリスト (画像/動画/PDF のみ)

## 入力バリデーション
- 全APIパラメータを axum extractor + serde で検証する
- ユーザー入力を直接ファイルパスやコマンドに組み込まない
- **scope パラメータ検証**（`/api/search?scope={node_id}` 等、node_id からパス解決が必要な場合）:
  1. `NodeRegistry::resolve(node_id)` で絶対パスに解決（`&Path` → `to_path_buf()` 後にロック解放）
  2. `PathSecurity::validate_existing(abs_path)` で存在確認 + マウントポイント配下か検証
  3. `abs_path.metadata()?.is_dir()` でディレクトリ判定（ファイルなら 422）
  4. `NodeRegistry::compute_parent_path_key(abs_path)` で `{mount_id}/{relative}` プレフィックスを生成
- **LIKE クエリの scope_prefix**: `scope_prefix` 内の `\`, `%`, `_` をエスケープし bind parameter に `"{escaped}/%"` を渡す。SQL は `... LIKE ?N ESCAPE '\'`（文字列埋め込み禁止）

## ネットワーク
- デフォルトは `127.0.0.1` バインド (LAN公開防止)
- `0.0.0.0` は明示的な設定 + 認証が必要

## シークレット管理
- 認証トークン等は環境変数のみで管理
- ハードコード禁止
- `.env` は `.gitignore` に含める（含め済み）
- `NODE_SECRET` は本番/通常運用で必須の環境変数とする。テスト以外でのハードコード fallback は禁止（未設定時は panic + エラーメッセージ付き終了）

## 処理制限
- サムネ生成/PDF描画: タイムアウト + メモリ上限
- 検索/インデックス再構築: レート制限 + 排他制御
- CPUバウンド処理: `spawn_blocking` でイベントループをブロックしない
- 起動直後のリクエスト保護: readiness プローブ `/api/ready` で初回スキャン完了を判定。liveness は `/api/health` と分離（docker-compose healthcheck は readiness を使う）

## 依存関係
- 既知の脆弱性があるライブラリは使用しない
- 定期的に `cargo audit` / `cargo-deny` / `npm audit` を実行する
