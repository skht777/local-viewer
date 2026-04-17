//! Local Content Viewer — Rust バックエンド エントリポイント
//!
//! ローカルディレクトリの画像・動画・PDFを閲覧する Web アプリのバックエンド。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::middleware::from_fn;
use axum::response::IntoResponse;
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
use services::node_registry::{NodeRegistry, PopulateStats, populate_registry};
use services::path_security::PathSecurity;
use state::AppState;

/// 起動時 populate を skip する既定の上限エントリ数
const POPULATE_MAX_ENTRIES_DEFAULT: usize = 2_000_000;

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

/// `/api/health` に含める起動時 populate 統計
///
/// 縮退観測用: 再起動後 `node_id` deep link が正しく回復しているかを JSON で公開する
#[derive(Serialize)]
struct RegistryPopulateStats {
    registered: usize,
    skipped_missing_mount: usize,
    skipped_malformed: usize,
    skipped_traversal: usize,
    errors: usize,
    degraded: bool,
}

impl From<&PopulateStats> for RegistryPopulateStats {
    fn from(s: &PopulateStats) -> Self {
        Self {
            registered: s.registered,
            skipped_missing_mount: s.skipped_missing_mount,
            skipped_malformed: s.skipped_malformed,
            skipped_traversal: s.skipped_traversal,
            errors: s.errors,
            degraded: s.degraded,
        }
    }
}

#[derive(Serialize)]
struct HealthResponseWithStats {
    status: String,
    registry_populate: RegistryPopulateStats,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponseWithStats> {
    Json(HealthResponseWithStats {
        status: "ok".to_string(),
        registry_populate: RegistryPopulateStats::from(state.registry_populate_stats.as_ref()),
    })
}

/// Readiness プローブ
///
/// - warm start: `dir_index.is_ready()` が即 true → 200
/// - cold start: 全マウントスキャン完了後に `scan_complete` が true → 200
/// - いずれも false → 503
async fn ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let is_ready = state
        .scan_complete
        .load(std::sync::atomic::Ordering::Relaxed)
        || state.dir_index.is_ready();
    if is_ready {
        (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ready".to_string(),
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "warming_up".to_string(),
            }),
        )
    }
}

/// バックグラウンドタスク (ウォームスタート判定 + インデックススキャン) のコンテキスト
struct BackgroundContext {
    indexer: Arc<services::indexer::Indexer>,
    dir_index: Arc<services::dir_index::DirIndex>,
    path_security: Arc<PathSecurity>,
    mount_id_map: HashMap<String, PathBuf>,
    scan_workers: usize,
    scan_complete: Arc<std::sync::atomic::AtomicBool>,
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
    // mount_id_map は BackgroundContext へ move するため先に clone を渡す
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

    // 起動時 NodeRegistry populate（再起動後 deep link 回復）
    // - Indexer の永続エントリから {mount_id}/{rest} を列挙し、
    //   HMAC 冪等性で再起動前と同じ node_id を再生成して id_to_path に登録
    // - 閾値超えや list_entry_paths 失敗時は lazy フォールバックへ縮退
    let populate_stats = hydrate_node_registry(&mut registry, &indexer, &mount_id_map);

    // DirIndex 初期化
    let dir_index_path = settings.index_db_path.replace(".db", "-dir.db");
    let dir_index = Arc::new(services::dir_index::DirIndex::new(&dir_index_path));
    if let Err(e) = dir_index.init_db() {
        tracing::error!("DirIndex DB 初期化失敗: {e}");
    }

    // バックグラウンドタスク用コンテキストを先に構築
    let scan_complete = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let bg_context = BackgroundContext {
        indexer: Arc::clone(&indexer),
        dir_index: Arc::clone(&dir_index),
        path_security: Arc::clone(&path_security),
        mount_id_map,
        scan_workers: settings.scan_workers,
        scan_complete: Arc::clone(&scan_complete),
    };
    let app_state = Arc::new(AppState {
        settings: Arc::new(settings),
        node_registry: Arc::new(Mutex::new(registry)),
        archive_service,
        temp_file_cache,
        thumbnail_service,
        video_converter,
        thumbnail_warmer,
        thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(16)),
        archive_thumb_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
        indexer,
        dir_index,
        last_rebuild: tokio::sync::Mutex::new(None),
        scan_complete: Arc::clone(&scan_complete),
        registry_populate_stats: Arc::new(populate_stats),
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
        .route("/api/ready", get(ready))
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
        .route(
            "/api/browse/{parent_node_id}/siblings",
            get(routers::browse::find_siblings),
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

/// 起動時に `NodeRegistry` を `Indexer` の永続エントリから rehydrate する
///
/// - Indexer の `list_entry_paths` から `{mount_id}/{rest}` を取得
/// - 閾値 `POPULATE_MAX_ENTRIES`（環境変数、デフォルト 2,000,000）を超える場合は skip して縮退
/// - `list_entry_paths` 失敗時も縮退（`degraded = true`）し `build_app` は継続させる
fn hydrate_node_registry(
    registry: &mut NodeRegistry,
    indexer: &services::indexer::Indexer,
    mount_id_map: &HashMap<String, PathBuf>,
) -> PopulateStats {
    let start = std::time::Instant::now();
    let max_entries = std::env::var("POPULATE_MAX_ENTRIES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(POPULATE_MAX_ENTRIES_DEFAULT);

    let paths = match indexer.list_entry_paths() {
        Ok(paths) => paths,
        Err(e) => {
            tracing::error!("NodeRegistry populate: list_entry_paths 失敗: {e}");
            return PopulateStats {
                degraded: true,
                ..PopulateStats::default()
            };
        }
    };

    if paths.len() > max_entries {
        tracing::warn!(
            entries = paths.len(),
            max_entries,
            "NodeRegistry populate: 閾値超えのため skip（縮退モード）"
        );
        return PopulateStats {
            degraded: true,
            ..PopulateStats::default()
        };
    }

    let stats = populate_registry(registry, &paths, mount_id_map);
    let elapsed = start.elapsed();
    tracing::info!(
        registered = stats.registered,
        skipped_missing_mount = stats.skipped_missing_mount,
        skipped_malformed = stats.skipped_malformed,
        skipped_traversal = stats.skipped_traversal,
        errors = stats.errors,
        degraded = stats.degraded,
        elapsed_ms = elapsed.as_millis() as u64,
        "NodeRegistry populate 完了"
    );
    stats
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
    let watcher_dir_index = Arc::clone(&bg.dir_index);
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
                    let mut bulk = match dir_index.begin_bulk() {
                        Ok(b) => Some(b),
                        Err(e) => {
                            tracing::warn!("DirIndex begin_bulk 失敗 ({mount_id}): {e}");
                            None
                        }
                    };
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
                        if let Err(e) = bulk.flush() {
                            tracing::warn!("DirIndex flush 失敗 ({mount_id}): {e}");
                        }
                    }
                    if let Err(e) = &result {
                        tracing::error!("Incremental scan 失敗 ({mount_id}): {e}");
                    }
                } else {
                    // フルスキャン + DirIndex コールバック
                    let mut bulk = match dir_index.begin_bulk() {
                        Ok(b) => Some(b),
                        Err(e) => {
                            tracing::warn!("DirIndex begin_bulk 失敗 ({mount_id}): {e}");
                            None
                        }
                    };
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
                        if let Err(e) = bulk.flush() {
                            tracing::warn!("DirIndex flush 失敗 ({mount_id}): {e}");
                        }
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
        bg.scan_complete
            .store(true, std::sync::atomic::Ordering::Relaxed);
    });

    // スキャン完了後に FileWatcher を起動
    tokio::spawn(async move {
        let _ = scan_handle.await;
        tracing::info!("全マウントのスキャン完了、FileWatcher を起動");

        let file_watcher = services::file_watcher::FileWatcher::new(
            watcher_indexer,
            watcher_path_security,
            watcher_dir_index,
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
