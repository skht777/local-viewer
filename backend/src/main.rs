//! Local Content Viewer — Rust バックエンド エントリポイント
//!
//! ローカルディレクトリの画像・動画・PDF を閲覧する Web アプリのバックエンド。
//! 起動時処理は `bootstrap` モジュールに委譲し、`main` は CLI パースと軸の起動のみに絞る。

use std::net::SocketAddr;

use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod bootstrap;
mod config;
mod errors;
mod middleware;
mod routers;
mod services;
mod state;

use bootstrap::{background_tasks::spawn_background_tasks, build_app};
use config::Settings;

/// CLI 引数
#[derive(Parser, Debug)]
#[command(name = "local-viewer-backend", about = "Local Content Viewer backend")]
struct Cli {
    /// バインドポート
    #[arg(long, default_value = "8000")]
    port: u16,

    /// バインドアドレス
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();

    let settings = Settings::new().map_err(|e| anyhow::anyhow!("設定エラー: {e}"))?;
    let (app, bg) = build_app(settings)?;

    // ウォームスタート判定 + バックグラウンドスキャン起動
    spawn_background_tasks(bg);

    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port).parse()?;
    tracing::info!("サーバー起動: {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::{Json, Router};
    use serde::Serialize;
    use tower::ServiceExt;

    use crate::bootstrap::{health, ready};
    use crate::config::Settings;
    use crate::routers;
    use crate::services::dir_index::DirIndex;
    use crate::services::node_registry::{NodeRegistry, PopulateStats, populate_registry};
    use crate::services::path_security::PathSecurity;
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;
    use crate::services::{self};
    use crate::state::AppState;

    // health / ready はテストから直接参照できるよう bootstrap から再エクスポート済み
    #[allow(dead_code, reason = "Serialize 実装のテスト用に残す")]
    #[derive(Serialize)]
    struct HealthResponse {
        status: String,
    }

    fn test_app() -> Router {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        // TempDir を leak して test 中に消えないようにする
        std::mem::forget(dir);

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().into_owned(),
        )]))
        .unwrap();

        let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, HashMap::new());
        let archive_service = Arc::new(services::archive::ArchiveService::new(&settings));
        let temp_file_cache = Arc::new(
            TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
        );
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));

        let index_db = tempfile::NamedTempFile::new().unwrap();
        let indexer = Arc::new(services::indexer::Indexer::new(
            index_db.path().to_str().unwrap(),
        ));
        indexer.init_db().unwrap();

        let dir_index_db = tempfile::NamedTempFile::new().unwrap();
        let dir_index = Arc::new(DirIndex::new(dir_index_db.path().to_str().unwrap()));
        dir_index.init_db().unwrap();

        let app_state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
            archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
            scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            registry_populate_stats: Arc::new(PopulateStats::default()),
            last_scan_report: Arc::new(std::sync::RwLock::new(None)),
            rebuild_guard: Arc::new(crate::services::rebuild_guard::RebuildGuard::new()),
        });

        Router::new()
            .route("/api/health", get(health))
            .route("/api/ready", get(ready))
            .route("/api/mounts", get(routers::mounts::list_mounts))
            .with_state(app_state)
    }

    // Json は bootstrap/api_router 側で使用するため、テスト内の JSON バリデーションで
    // serde_json を使うだけで十分 (型は再定義不要)
    #[allow(dead_code, reason = "serde_json で JSON 経由で検証するため未使用")]
    fn _ensure_json_imported() -> Json<HealthResponse> {
        Json(HealthResponse {
            status: "ok".to_string(),
        })
    }

    #[tokio::test]
    async fn ヘルスチェックが200を返す() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ヘルスレスポンスにregistry_populate統計が含まれる() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let stats = json
            .get("registry_populate")
            .expect("registry_populate セクションが存在する");
        assert!(stats.get("registered").is_some());
        assert!(stats.get("skipped_missing_mount").is_some());
        assert!(stats.get("skipped_malformed").is_some());
        assert!(stats.get("skipped_traversal").is_some());
        assert!(stats.get("errors").is_some());
        assert!(stats.get("degraded").is_some());
    }

    #[tokio::test]
    async fn マウント一覧が200を返す() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/mounts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- /api/health の last_scan diagnostics 回帰テスト ---

    use crate::services::scan_diagnostics::{FingerprintAction, MountDiagnostic, ScanDiagnostics};

    /// `last_scan_report` を事前に書き込んだ `Router` を作る
    fn test_app_with_report(report: Option<ScanDiagnostics>) -> Router {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::mem::forget(dir);
        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().into_owned(),
        )]))
        .unwrap();
        let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, HashMap::new());
        let archive_service = Arc::new(services::archive::ArchiveService::new(&settings));
        let temp_file_cache = Arc::new(
            TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
        );
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));
        let index_db = tempfile::NamedTempFile::new().unwrap();
        let indexer = Arc::new(services::indexer::Indexer::new(
            index_db.path().to_str().unwrap(),
        ));
        indexer.init_db().unwrap();
        let dir_index_db = tempfile::NamedTempFile::new().unwrap();
        let dir_index = Arc::new(DirIndex::new(dir_index_db.path().to_str().unwrap()));
        dir_index.init_db().unwrap();
        let last_scan_report = Arc::new(std::sync::RwLock::new(report.map(Arc::new)));
        let app_state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
            archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
            scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            registry_populate_stats: Arc::new(PopulateStats::default()),
            last_scan_report,
            rebuild_guard: Arc::new(crate::services::rebuild_guard::RebuildGuard::new()),
        });
        Router::new()
            .route("/api/health", get(health))
            .with_state(app_state)
    }

    /// 事前に `RwLock` を poison 状態にした `Router` を作る
    ///
    /// `catch_unwind` で write guard を握ったまま panic させ、poison フラグを立てる。
    /// liveness 契約 (/api/health は常時 200) の耐性を検証するため
    fn test_app_with_poisoned_report() -> Router {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::mem::forget(dir);
        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().into_owned(),
        )]))
        .unwrap();
        let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, HashMap::new());
        let archive_service = Arc::new(services::archive::ArchiveService::new(&settings));
        let temp_file_cache = Arc::new(
            TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
        );
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));
        let index_db = tempfile::NamedTempFile::new().unwrap();
        let indexer = Arc::new(services::indexer::Indexer::new(
            index_db.path().to_str().unwrap(),
        ));
        indexer.init_db().unwrap();
        let dir_index_db = tempfile::NamedTempFile::new().unwrap();
        let dir_index = Arc::new(DirIndex::new(dir_index_db.path().to_str().unwrap()));
        dir_index.init_db().unwrap();
        let last_scan_report: Arc<std::sync::RwLock<Option<Arc<ScanDiagnostics>>>> =
            Arc::new(std::sync::RwLock::new(None));
        // catch_unwind で poison 状態を作る
        let poisoner = Arc::clone(&last_scan_report);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = poisoner.write().unwrap();
            panic!("intentional poison for test");
        }));
        assert!(last_scan_report.is_poisoned(), "RwLock should be poisoned");
        let app_state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
            archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
            scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            registry_populate_stats: Arc::new(PopulateStats::default()),
            last_scan_report,
            rebuild_guard: Arc::new(crate::services::rebuild_guard::RebuildGuard::new()),
        });
        Router::new()
            .route("/api/health", get(health))
            .with_state(app_state)
    }

    async fn fetch_last_scan(app: Router) -> (StatusCode, serde_json::Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    fn all_ok_report() -> ScanDiagnostics {
        ScanDiagnostics {
            completed_at_ms: 1_700_000_000_000,
            is_warm_start: false,
            cleanup_ok: true,
            scans_ok: true,
            all_ok: true,
            fingerprint: FingerprintAction::Saved,
            mounts: vec![MountDiagnostic {
                mount_id: "0123456789abcdef".to_string(),
                scan_ok: true,
                dir_index_ok: true,
                panicked: false,
                walk: None,
            }],
        }
    }

    #[tokio::test]
    async fn healthルートはlast_scan未書き込み時にlast_scan_nullを返す() {
        let app = test_app_with_report(None);
        let (status, json) = fetch_last_scan(app).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            json.get("last_scan")
                .is_some_and(serde_json::Value::is_null),
            "last_scan は null (未完了 or panic): {json}"
        );
    }

    #[tokio::test]
    #[allow(
        non_snake_case,
        reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
    )]
    async fn healthルートはall_ok時にScanDiagnosticsをJSON化して返す() {
        let app = test_app_with_report(Some(all_ok_report()));
        let (status, json) = fetch_last_scan(app).await;
        assert_eq!(status, StatusCode::OK);
        let last = json.get("last_scan").expect("last_scan 存在");
        assert_eq!(last.get("cleanup_ok"), Some(&serde_json::json!(true)));
        assert_eq!(last.get("scans_ok"), Some(&serde_json::json!(true)));
        assert_eq!(last.get("all_ok"), Some(&serde_json::json!(true)));
        assert_eq!(last.get("is_warm_start"), Some(&serde_json::json!(false)));
        assert_eq!(last.get("fingerprint"), Some(&serde_json::json!("saved")));
        let mounts = last.get("mounts").and_then(|v| v.as_array()).unwrap();
        assert_eq!(mounts.len(), 1);
        assert_eq!(
            mounts[0].get("mount_id"),
            Some(&serde_json::json!("0123456789abcdef"))
        );
    }

    #[tokio::test]
    async fn healthルートはcold_partial時にall_ok_falseとfingerprint_not_neededを返す() {
        let report = ScanDiagnostics {
            cleanup_ok: true,
            scans_ok: false,
            all_ok: false,
            is_warm_start: false,
            fingerprint: FingerprintAction::NotNeeded,
            mounts: vec![MountDiagnostic {
                mount_id: "0123456789abcdef".to_string(),
                scan_ok: false,
                dir_index_ok: true,
                panicked: false,
                walk: None,
            }],
            ..all_ok_report()
        };
        let app = test_app_with_report(Some(report));
        let (status, json) = fetch_last_scan(app).await;
        assert_eq!(status, StatusCode::OK);
        let last = json.get("last_scan").unwrap();
        assert_eq!(last.get("all_ok"), Some(&serde_json::json!(false)));
        assert_eq!(last.get("scans_ok"), Some(&serde_json::json!(false)));
        assert_eq!(
            last.get("fingerprint"),
            Some(&serde_json::json!("not_needed"))
        );
    }

    #[tokio::test]
    async fn healthルートはwarm_partial_cleared時にfingerprint_clearedを返す() {
        let report = ScanDiagnostics {
            cleanup_ok: true,
            scans_ok: false,
            all_ok: false,
            is_warm_start: true,
            fingerprint: FingerprintAction::Cleared,
            mounts: vec![MountDiagnostic {
                mount_id: "0123456789abcdef".to_string(),
                scan_ok: true,
                dir_index_ok: false,
                panicked: false,
                walk: None,
            }],
            ..all_ok_report()
        };
        let app = test_app_with_report(Some(report));
        let (status, json) = fetch_last_scan(app).await;
        assert_eq!(status, StatusCode::OK);
        let last = json.get("last_scan").unwrap();
        assert_eq!(last.get("is_warm_start"), Some(&serde_json::json!(true)));
        assert_eq!(last.get("all_ok"), Some(&serde_json::json!(false)));
        assert_eq!(last.get("fingerprint"), Some(&serde_json::json!("cleared")));
    }

    #[tokio::test]
    async fn healthルートはwarm_partial_clear_failed時にfingerprint_clear_failedを返す() {
        let report = ScanDiagnostics {
            cleanup_ok: true,
            scans_ok: false,
            all_ok: false,
            is_warm_start: true,
            fingerprint: FingerprintAction::ClearFailed,
            mounts: vec![MountDiagnostic {
                mount_id: "0123456789abcdef".to_string(),
                scan_ok: true,
                dir_index_ok: false,
                panicked: false,
                walk: None,
            }],
            ..all_ok_report()
        };
        let app = test_app_with_report(Some(report));
        let (status, json) = fetch_last_scan(app).await;
        assert_eq!(status, StatusCode::OK);
        let last = json.get("last_scan").unwrap();
        assert_eq!(
            last.get("fingerprint"),
            Some(&serde_json::json!("clear_failed"))
        );
    }

    #[tokio::test]
    async fn healthルートはlast_scan_report_poison時にlast_scan_nullでpanicしない() {
        let app = test_app_with_poisoned_report();
        let (status, json) = fetch_last_scan(app).await;
        assert_eq!(status, StatusCode::OK, "liveness 契約を守る");
        assert!(
            json.get("last_scan")
                .is_some_and(serde_json::Value::is_null),
            "poison 時は last_scan=null fallback: {json}"
        );
    }

    // --- 再起動後の deep link 回復（populate_registry 経路）回帰テスト ---
    //
    // 想定シナリオ: 前セッションで Indexer に永続化された {mount_id}/{rest} を元に、
    // 新しい NodeRegistry を populate_registry で rehydrate → 古い node_id が
    // そのまま各エンドポイントで解決できることを検証する（HMAC 冪等性）。

    use crate::services::indexer::IndexEntry;

    /// 非画像エントリを含む warm-start 相当の `AppState` を構築する
    ///
    /// - `TempDir` にサブディレクトリ / .mp4 / .pdf を配置
    /// - 元の `NodeRegistry` で `register_resolved` して想定 `node_id` を取得
    /// - `Indexer` に `add_entry` で `{mount_id}/{rest}` を登録
    /// - 新しい `NodeRegistry` を作って `populate_registry` → 同じ `node_id` が再生成されることを確認
    #[allow(
        clippy::type_complexity,
        clippy::too_many_lines,
        clippy::similar_names,
        reason = "テストヘルパー: warm-start セットアップを一箇所にまとめる"
    )]
    fn warm_state() -> (Arc<AppState>, WarmTargets) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::mem::forget(dir);

        // サンプルエントリ（非画像）を配置
        std::fs::create_dir_all(root.join("videos")).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("videos/clip.mp4"), b"\x00\x00\x00\x20ftypmp42").unwrap();
        std::fs::write(root.join("docs/manual.pdf"), b"%PDF-1.4\n").unwrap();

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().into_owned(),
        )]))
        .unwrap();

        let mut mount_names = HashMap::new();
        mount_names.insert(root.clone(), "test_mount".to_string());
        let mut mount_id_map = HashMap::new();
        mount_id_map.insert("test_mount".to_string(), root.clone());

        let ps = Arc::new(PathSecurity::new(vec![root.clone()], false).unwrap());

        // Step 1: 元の NodeRegistry で node_id を生成（前セッション相当）
        let dir_id = {
            let mut r = NodeRegistry::new(Arc::clone(&ps), 100_000, mount_names.clone());
            r.set_mount_id_map(mount_id_map.clone());
            r.register(&root.join("videos")).unwrap()
        };
        let video_id = {
            let mut r = NodeRegistry::new(Arc::clone(&ps), 100_000, mount_names.clone());
            r.set_mount_id_map(mount_id_map.clone());
            r.register(&root.join("videos/clip.mp4")).unwrap()
        };
        let pdf_id = {
            let mut r = NodeRegistry::new(Arc::clone(&ps), 100_000, mount_names.clone());
            r.set_mount_id_map(mount_id_map.clone());
            r.register(&root.join("docs/manual.pdf")).unwrap()
        };

        // Step 2: Indexer に永続化（前セッションのスキャン結果相当）
        // NamedTempFile は drop で unlink されるため、TempDir 内の固定名ファイルを使う
        let db_dir = tempfile::TempDir::new().unwrap().keep();
        let index_db_path = db_dir.join("index.db");
        let indexer = Arc::new(services::indexer::Indexer::new(
            index_db_path.to_str().unwrap(),
        ));
        indexer.init_db().unwrap();
        // テストでは warm-start 相当として is_ready フラグを立てておく
        indexer.mark_warm_start();
        for (rel, name, kind) in [
            ("test_mount/videos", "videos", "directory"),
            ("test_mount/videos/clip.mp4", "clip.mp4", "video"),
            ("test_mount/docs/manual.pdf", "manual.pdf", "pdf"),
        ] {
            indexer
                .add_entry(&IndexEntry {
                    relative_path: rel.to_string(),
                    name: name.to_string(),
                    kind: kind.to_string(),
                    size_bytes: Some(128),
                    mtime_ns: 1_000_000_000,
                })
                .unwrap();
        }

        // Step 3: 新しい NodeRegistry を populate_registry で rehydrate
        let mut registry = NodeRegistry::new(Arc::clone(&ps), 100_000, mount_names);
        registry.set_mount_id_map(mount_id_map.clone());
        let paths = indexer.list_entry_paths().unwrap();
        let stats = populate_registry(&mut registry, &paths, &mount_id_map);
        assert_eq!(stats.registered, 3);
        assert_eq!(stats.errors, 0);

        // 新 registry で同じ node_id が引けることを確認
        assert_eq!(
            registry
                .path_to_id_get(&root.join("videos").to_string_lossy())
                .unwrap(),
            dir_id
        );

        // AppState を組み立てる
        let archive_service = Arc::new(services::archive::ArchiveService::new(&settings));
        let temp_file_cache = Arc::new(
            TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
        );
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));

        let dir_index_path = db_dir.join("dir_index.db");
        let dir_index = Arc::new(DirIndex::new(dir_index_path.to_str().unwrap()));
        dir_index.init_db().unwrap();

        let state = Arc::new(AppState {
            settings: Arc::new(settings),
            node_registry: Arc::new(Mutex::new(registry)),
            archive_service,
            temp_file_cache,
            thumbnail_service,
            video_converter,
            thumbnail_warmer,
            thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
            archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
            scan_complete: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            registry_populate_stats: Arc::new(stats),
            last_scan_report: Arc::new(std::sync::RwLock::new(None)),
            rebuild_guard: Arc::new(crate::services::rebuild_guard::RebuildGuard::new()),
        });

        (
            state,
            WarmTargets {
                dir_id,
                video_id,
                pdf_id,
            },
        )
    }

    #[allow(
        clippy::struct_field_names,
        reason = "各フィールドは異なる node_id を表すため _id 付きで統一"
    )]
    struct WarmTargets {
        dir_id: String,
        video_id: String,
        #[allow(dead_code, reason = "PDF の deep link テスト用")]
        pdf_id: String,
    }

    fn warm_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route(
                "/api/browse/{node_id}",
                get(routers::browse::browse_directory),
            )
            .route("/api/file/{node_id}", get(routers::file::serve_file))
            .route(
                "/api/thumbnail/{node_id}",
                get(routers::thumbnail::serve_thumbnail),
            )
            .route("/api/search", get(routers::search::search))
            .with_state(state)
    }

    #[tokio::test]
    async fn 再起動後にbrowse_deep_linkが200を返す() {
        let (state, targets) = warm_state();
        let app = warm_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/browse/{}", targets.dir_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn 再起動後にfile_deep_linkが200を返す() {
        let (state, targets) = warm_state();
        let app = warm_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/file/{}", targets.video_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn 再起動後にthumbnail_deep_linkが成功する() {
        let (state, targets) = warm_state();
        let app = warm_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/thumbnail/{}", targets.video_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // サムネイル生成は ffmpeg 有無に依存するため 200/500 どちらでも許容。
        // 404（未登録 node_id）ではないこと＝rehydrate が効いていることを検証する。
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn 再起動後にsearch_scope_deep_linkが200を返す() {
        let (state, targets) = warm_state();
        let app = warm_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/search?q=clip&scope={}", targets.dir_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
