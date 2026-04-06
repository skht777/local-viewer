//! Local Content Viewer — Rust バックエンド エントリポイント
//!
//! ローカルディレクトリの画像・動画・PDFを閲覧する Web アプリのバックエンド。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::http::Method;
use axum::middleware::from_fn;
use axum::{
    Json, Router,
    routing::{get, post},
};
use clap::Parser;
use serde::Serialize;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod config;
mod errors;
mod middleware;
mod routers;
mod services;
mod state;

use config::Settings;
use services::mount_config::load_mount_config;
use services::node_registry::NodeRegistry;
use services::path_security::PathSecurity;
use state::AppState;

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

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// バックグラウンドタスク (ウォームスタート判定 + インデックススキャン) のコンテキスト
struct BackgroundContext {
    indexer: Arc<services::indexer::Indexer>,
    dir_index: Arc<services::dir_index::DirIndex>,
    path_security: Arc<PathSecurity>,
    mount_id_map: HashMap<String, PathBuf>,
    scan_workers: usize,
}

/// サービス初期化 + ルーター構築
///
/// 初期化順序:
/// 1. `Settings::new()` — 環境変数パース
/// 2. `load_mount_config()` — mounts.json 読み込み
/// 3. `MountPoint::resolve_path()` — `root_dirs` 構築
/// 4. `PathSecurity::new()` — パストラバーサル防止
/// 5. `NodeRegistry::new()` — HMAC `node_id` マッピング
/// 6. `AppState` 構築
/// 7. ルーター + ミドルウェア登録
#[allow(clippy::too_many_lines, reason = "サービス初期化は一箇所にまとめる")]
fn build_app(settings: Settings) -> anyhow::Result<(Router, BackgroundContext)> {
    // マウントポイント設定読み込み
    let config_path = PathBuf::from(&settings.mount_config_path);
    let config = load_mount_config(&config_path, &settings.mount_base_dir)
        .map_err(|e| anyhow::anyhow!("マウント設定の読み込みに失敗: {e}"))?;

    // root_dirs, mount_names, mount_id_map を構築
    let mut root_dirs = Vec::new();
    let mut mount_names: HashMap<PathBuf, String> = HashMap::new();
    let mut mount_id_map: HashMap<String, PathBuf> = HashMap::new();

    if config.mounts.is_empty() {
        // マウント定義なし → base_dir 自体をルートとして使用
        let base_resolved = std::fs::canonicalize(&settings.mount_base_dir)
            .unwrap_or_else(|_| settings.mount_base_dir.clone());
        tracing::warn!(
            "マウント定義がありません。base_dir をルートとして使用: {}",
            base_resolved.display()
        );
        root_dirs.push(base_resolved);
    } else {
        for mp in &config.mounts {
            match mp.resolve_path(&settings.mount_base_dir) {
                Ok(resolved) => {
                    mount_names.insert(resolved.clone(), mp.name.clone());
                    mount_id_map.insert(mp.mount_id.clone(), resolved.clone());
                    root_dirs.push(resolved);
                }
                Err(e) => {
                    tracing::warn!(
                        "マウントポイント '{}' (slug={}) の解決に失敗: {e}",
                        mp.name,
                        mp.slug
                    );
                }
            }
        }
    }

    if root_dirs.is_empty() {
        anyhow::bail!("有効なマウントポイントがありません");
    }

    tracing::info!("マウントポイント: {} 件 ({:?})", root_dirs.len(), root_dirs);

    // サービス初期化
    let path_security = Arc::new(PathSecurity::new(root_dirs, settings.is_allow_symlinks)?);
    let mut registry = NodeRegistry::new(
        Arc::clone(&path_security),
        settings.archive_registry_max_entries,
        mount_names,
    );
    registry.set_mount_id_map(mount_id_map.clone());

    // アーカイブサービス構築 + diagnostics ログ
    let archive_service = Arc::new(services::archive::ArchiveService::new(&settings));
    let diag = archive_service.get_diagnostics();
    tracing::info!("アーカイブサポート: {:?}", diag);

    // サムネイル・動画関連サービス構築
    let cache_dir = std::env::temp_dir().join("viewer-disk-cache");
    let cache_max = u64::from(settings.archive_disk_cache_mb) * 1024 * 1024;
    let temp_file_cache = Arc::new(
        services::temp_file_cache::TempFileCache::new(cache_dir, cache_max)
            .map_err(|e| anyhow::anyhow!("ディスクキャッシュ初期化失敗: {e}"))?,
    );
    let video_converter = Arc::new(services::video_converter::VideoConverter::new(
        Arc::clone(&temp_file_cache),
        &settings,
    ));
    if video_converter.is_available() {
        tracing::info!("Video remux: FFmpeg available");
    } else {
        tracing::warn!("Video remux: FFmpeg not found");
    }
    let thumbnail_service = Arc::new(services::thumbnail_service::ThumbnailService::new(
        Arc::clone(&temp_file_cache),
    ));
    let thumbnail_warmer = Arc::new(services::thumbnail_warmer::ThumbnailWarmer::new(4));

    // 検索インデクサー初期化
    let indexer = Arc::new(services::indexer::Indexer::new(&settings.index_db_path));
    if let Err(e) = indexer.init_db() {
        tracing::error!("インデックス DB 初期化失敗: {e}");
    }

    // DirIndex 初期化
    let dir_index_path = settings.index_db_path.replace(".db", "-dir.db");
    let dir_index = Arc::new(services::dir_index::DirIndex::new(&dir_index_path));
    if let Err(e) = dir_index.init_db() {
        tracing::error!("DirIndex DB 初期化失敗: {e}");
    }

    // バックグラウンドタスク用コンテキストを先に構築
    let bg_context = BackgroundContext {
        indexer: Arc::clone(&indexer),
        dir_index: Arc::clone(&dir_index),
        path_security: Arc::clone(&path_security),
        mount_id_map: mount_id_map.clone(),
        scan_workers: settings.scan_workers,
    };

    let app_state = Arc::new(AppState {
        settings: Arc::new(settings),
        node_registry: Arc::new(Mutex::new(registry)),
        archive_service,
        temp_file_cache,
        thumbnail_service,
        video_converter,
        thumbnail_warmer,
        indexer,
        dir_index,
        last_rebuild: tokio::sync::Mutex::new(None),
    });

    // CORS: 開発用ポートを許可
    #[allow(clippy::expect_used, reason = "定数文字列のパースは失敗しない")]
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:5173".parse().expect("valid origin"),
            "http://localhost:5174".parse().expect("valid origin"),
        ])
        .allow_methods([Method::GET])
        .allow_headers(Any);

    // API ルーター構築
    let api_router = Router::new()
        .route("/api/health", get(health))
        .route("/api/mounts", get(routers::mounts::list_mounts))
        .route(
            "/api/browse/{node_id}",
            get(routers::browse::browse_directory),
        )
        .route(
            "/api/browse/{node_id}/first-viewable",
            get(routers::browse::first_viewable),
        )
        .route(
            "/api/browse/{parent_node_id}/sibling",
            get(routers::browse::find_sibling),
        )
        .route("/api/file/{node_id}", get(routers::file::serve_file))
        .route(
            "/api/thumbnail/{node_id}",
            get(routers::thumbnail::serve_thumbnail),
        )
        .route(
            "/api/thumbnails/batch",
            post(routers::thumbnail::serve_thumbnails_batch),
        )
        .route("/api/search", get(routers::search::search))
        .route("/api/index/rebuild", post(routers::search::rebuild_index))
        .with_state(app_state);

    // 静的ファイル配信 + SPA フォールバック
    let app = attach_static_files(api_router);

    // ミドルウェア層 (レスポンスパス: Handler → CORS → SkipGzipBinary → Compression → Trace)
    let app = app
        .layer(cors)
        .layer(from_fn(middleware::skip_gzip_binary::skip_gzip_for_binary))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    Ok((app, bg_context))
}

/// 静的ファイル配信 + SPA フォールバックを追加する
///
/// Docker 本番環境 (static/ が存在する場合) のみ有効。
/// 開発時は Vite dev server がフロントエンドを処理する。
fn attach_static_files(router: Router) -> Router {
    let static_dir = std::env::current_dir().unwrap_or_default().join("static");

    if !static_dir.exists() {
        return router;
    }

    tracing::info!("静的ファイル配信有効: {}", static_dir.display());

    // /assets/* — Vite ハッシュ付きアセット (immutable 長期キャッシュ)
    let assets_service = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
        ))
        .service(ServeDir::new(static_dir.join("assets")));

    // SPA フォールバック: non-API パスは index.html を返す
    let spa_fallback =
        ServeDir::new(&static_dir).not_found_service(ServeFile::new(static_dir.join("index.html")));

    router
        .nest_service("/assets", assets_service)
        .fallback_service(spa_fallback)
}

/// ウォームスタート判定 + バックグラウンドインデックススキャンを起動する
///
/// - ウォームスタート条件: Indexer にエントリあり + `DirIndex` フルスキャン完了 + マウントフィンガープリント一致
/// - マウントごとに `spawn_blocking` タスクを逐次実行 (`SQLite` 並行書き込み競合を回避)
/// - 全マウント完了後、`DirIndex` のフラグ設定 + `FileWatcher` 起動
#[allow(
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    reason = "スキャン起動の分岐ロジックは一箇所にまとめる、Arc フィールドを spawn に移動するため所有権が必要"
)]
fn spawn_background_tasks(bg: BackgroundContext) {
    let mount_ids: Vec<String> = bg.mount_id_map.keys().cloned().collect();
    let mount_id_refs: Vec<&str> = mount_ids.iter().map(String::as_str).collect();

    let is_warm_start = bg.indexer.entry_count().unwrap_or(0) > 0
        && bg.dir_index.is_full_scan_done().unwrap_or(false)
        && bg
            .indexer
            .check_mount_fingerprint(&mount_id_refs)
            .unwrap_or(false);

    if is_warm_start {
        tracing::info!("Warm Start: 既存インデックスを使用");
        bg.indexer.mark_warm_start();
        bg.dir_index.mark_warm_start();
    } else {
        tracing::info!("フルスキャンを開始");
    }

    // マウントフィンガープリントを保存
    if let Err(e) = bg.indexer.save_mount_fingerprint(&mount_id_refs) {
        tracing::error!("マウントフィンガープリント保存失敗: {e}");
    }

    // マウントごとの逐次バックグラウンドスキャン (SQLite 並行書き込み競合を回避)
    let scan_mounts: Vec<(String, PathBuf)> = bg
        .mount_id_map
        .iter()
        .map(|(id, p)| (id.clone(), p.clone()))
        .collect();
    let mount_count = scan_mounts.len().max(1);
    let workers_per_mount = (bg.scan_workers / mount_count).max(2);

    let scan_dir_index = Arc::clone(&bg.dir_index);

    // FileWatcher 用に先に clone (bg は scan_handle の async move で消費されるため)
    let watcher_indexer = Arc::clone(&bg.indexer);
    let watcher_path_security = Arc::clone(&bg.path_security);
    let watcher_mounts: Vec<(String, PathBuf)> = bg
        .mount_id_map
        .iter()
        .map(|(id, p)| (id.clone(), p.clone()))
        .collect();

    // マウントごとのスキャンを逐次実行
    // Indexer と DirIndex は同一 SQLite DB を共有するため、並行書き込みで
    // SQLITE_BUSY によるトランザクション失敗を回避する
    let scan_handle = tokio::spawn(async move {
        for (mount_id, root) in scan_mounts {
            let indexer = Arc::clone(&bg.indexer);
            let dir_index = Arc::clone(&bg.dir_index);
            let path_security = Arc::clone(&bg.path_security);

            let result = tokio::task::spawn_blocking(move || {
                if is_warm_start {
                    // 差分スキャン + DirIndex コールバック
                    let mut bulk = dir_index.begin_bulk().ok();
                    let result = indexer.incremental_scan(
                        &root,
                        &path_security,
                        &mount_id,
                        workers_per_mount,
                        Some(&mut |args| {
                            if let Some(bulk) = bulk.as_mut() {
                                let _ = bulk.ingest_walk_entry(&args);
                            }
                        }),
                    );
                    if let Some(mut bulk) = bulk {
                        let _ = bulk.flush();
                    }
                    if let Err(e) = &result {
                        tracing::error!("Incremental scan 失敗 ({mount_id}): {e}");
                    }
                } else {
                    // フルスキャン + DirIndex コールバック
                    let mut bulk = dir_index.begin_bulk().ok();
                    let result = indexer.scan_directory(
                        &root,
                        &path_security,
                        &mount_id,
                        workers_per_mount,
                        Some(&mut |args| {
                            if let Some(bulk) = bulk.as_mut() {
                                let _ = bulk.ingest_walk_entry(&args);
                            }
                        }),
                    );
                    if let Some(mut bulk) = bulk {
                        let _ = bulk.flush();
                    }
                    if let Err(e) = &result {
                        tracing::error!("Full scan 失敗 ({mount_id}): {e}");
                    }
                }
                tracing::info!("スキャン完了: {mount_id}");
            })
            .await;

            if let Err(e) = result {
                tracing::error!("バックグラウンドスキャンタスクがパニック: {e}");
            }
        }

        // 全マウントスキャン完了後にフラグを設定
        if !is_warm_start {
            let _ = scan_dir_index.mark_full_scan_done();
        }
        scan_dir_index.mark_ready();
    });

    // スキャン完了後に FileWatcher を起動
    tokio::spawn(async move {
        let _ = scan_handle.await;
        tracing::info!("全マウントのスキャン完了、FileWatcher を起動");

        let file_watcher = services::file_watcher::FileWatcher::new(
            watcher_indexer,
            watcher_path_security,
            watcher_mounts,
        );
        if let Err(e) = file_watcher.start() {
            tracing::error!("FileWatcher 起動失敗: {e}");
        }
        // FileWatcher を維持 (drop で停止するため)
        std::mem::forget(file_watcher);
    });
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
    use tower::ServiceExt;

    use crate::services::dir_index::DirIndex;
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;

    use super::*;

    fn test_app() -> Router {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        // TempDir を leak して test 中に消えないようにする
        std::mem::forget(dir);

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().to_string(),
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
            indexer,
            dir_index,
            last_rebuild: tokio::sync::Mutex::new(None),
        });

        Router::new()
            .route("/api/health", get(health))
            .route("/api/mounts", get(routers::mounts::list_mounts))
            .with_state(app_state)
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
}
