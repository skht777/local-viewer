//! マウント解決 + サービス初期化 + `AppState` 構築 + `NodeRegistry` rehydrate

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::config::Settings;
use crate::services::{
    self,
    dir_index::DirIndex,
    indexer::Indexer,
    mount_config::load_mount_config,
    node_registry::{NodeRegistry, PopulateStats, populate_registry},
    path_security::PathSecurity,
};
use crate::state::AppState;

use super::BackgroundContext;

/// 起動時 populate を skip する既定の上限エントリ数
const POPULATE_MAX_ENTRIES_DEFAULT: usize = 2_000_000;

/// マウント設定読込〜`AppState` 構築〜`BackgroundContext` 生成
#[allow(
    clippy::too_many_lines,
    reason = "サービス初期化は依存順が密でまとめておく"
)]
pub(crate) fn build_state(
    settings: Settings,
) -> anyhow::Result<(Arc<AppState>, BackgroundContext)> {
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
    let indexer = Arc::new(Indexer::new(&settings.index_db_path));
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
    let dir_index = Arc::new(DirIndex::new(&dir_index_path));
    if let Err(e) = dir_index.init_db() {
        tracing::error!("DirIndex DB 初期化失敗: {e}");
    }

    let scan_complete = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let last_scan_report = Arc::new(std::sync::RwLock::new(None));
    let rebuild_guard = Arc::new(services::rebuild_guard::RebuildGuard::new());
    let file_watcher = Arc::new(Mutex::new(None));

    let bg_context = BackgroundContext {
        indexer: Arc::clone(&indexer),
        dir_index: Arc::clone(&dir_index),
        path_security: Arc::clone(&path_security),
        mount_id_map,
        scan_workers: settings.scan_workers,
        scan_complete: Arc::clone(&scan_complete),
        last_scan_report: Arc::clone(&last_scan_report),
        rebuild_guard: Arc::clone(&rebuild_guard),
        file_watcher: Arc::clone(&file_watcher),
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
        scan_complete,
        registry_populate_stats: Arc::new(populate_stats),
        last_scan_report,
        rebuild_guard,
        file_watcher,
        path_security: Arc::clone(&path_security),
    });

    Ok((app_state, bg_context))
}

/// 起動時に `NodeRegistry` を `Indexer` の永続エントリから rehydrate する
///
/// - `Indexer` の `list_entry_paths` から `{mount_id}/{rest}` を取得
/// - 閾値 `POPULATE_MAX_ENTRIES`（環境変数、デフォルト 2,000,000）を超える場合は skip して縮退
/// - `list_entry_paths` 失敗時も縮退（`degraded = true`）し `build_state` は継続させる
fn hydrate_node_registry(
    registry: &mut NodeRegistry,
    indexer: &Indexer,
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
