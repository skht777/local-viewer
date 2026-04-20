//! `mounts.json` hot reload サービス
//!
//! `POST /api/mounts/reload` から呼ばれ、mount 構成の差分を反映する。
//!
//! スコープ:
//! - **削除された mount のみ**サポート（DB 行と `NodeRegistry` / `PathSecurity`
//!   状態を同期的に取り除く）
//! - **追加された mount は `tracing::warn!` でスキップ**。`manage_mounts.sh` が
//!   `docker-compose.override.yml` のバインドマウントも同時編集するため、
//!   追加はコンテナ再起動（`./start.sh`）が必須。hot reload では受け付けない
//!
//! Lock 順序（deadlock 回避のため固定）:
//! 1. `state.node_registry.lock()` で旧 `mount_id_map` を取得し、削除対象を列挙
//! 2. lock 解放
//! 3. `spawn_blocking` 内で `perform_full_stale_cleanup` を実行（SQLite I/O）
//! 4. `state.node_registry.lock()` で `remove_mount` + `rebuild_root_entries_cache`
//! 5. `state.path_security.replace_roots(new_roots)` で内部 `RwLock` 差し替え
//! 6. `state.file_watcher.lock()` で旧 watcher stop → 新 watcher new/start → replace
//!
//! `rebuild_guard` の取得と解放は呼び出し側（router）の責務（RAII）。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::errors::AppError;
use crate::services::file_watcher::FileWatcher;
use crate::services::mount_cleanup::perform_full_stale_cleanup;
use crate::services::mount_config::MountConfig;
use crate::state::AppState;

/// reload 結果の診断 DTO
///
/// `POST /api/mounts/reload` のレスポンス body としてもそのまま返す。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct MountReloadResult {
    /// 新 config から消えた `mount_id`（stale cleanup + `NodeRegistry` から除去済み）
    pub removed: Vec<String>,
    /// 新 config にあり旧にない `mount_id`（docker restart が必要なため警告のみ）
    pub ignored_additions: Vec<String>,
    /// stale cleanup（`perform_full_stale_cleanup`）が全件成功したか
    pub cleanup_ok: bool,
}

/// mounts.json hot reload の本体
///
/// - 呼び出し側で `rebuild_guard` を取得してから呼ぶこと（RAII ハンドル保持）
/// - `new_config` はパース済み `MountConfig`。`resolve_path` はここで行う
/// - 戻り値の `cleanup_ok=false` は stale cleanup が部分失敗したことを示す。
///   その場合でも `NodeRegistry` / `PathSecurity` / `FileWatcher` は新構成に
///   切り替わる（DB 上の stale 行は残るが次回起動の cleanup で再試行できる）
#[allow(
    clippy::too_many_lines,
    reason = "hot reload 全体の lock 順序を 1 関数内で担保することに価値があるため分割しない"
)]
pub(crate) async fn reload_mounts(
    state: Arc<AppState>,
    new_config: MountConfig,
) -> Result<MountReloadResult, AppError> {
    // Step 0: shutdown 中なら即時 503 を返す（冒頭 check）
    if state.shutdown_token.is_cancelled() {
        return Err(AppError::ShutdownInProgress(
            "shutdown 中のため mounts reload を拒否".to_string(),
        ));
    }

    let base_dir = state.settings.mount_base_dir.clone();

    // Step 1: 新 config から (mount_id, resolved_root) ペアを構築
    //   resolve_path 失敗は 400 相当（mounts.json 不整合）として早期 return
    let new_entries: HashMap<String, PathBuf> = new_config
        .mounts
        .iter()
        .map(|m| Ok((m.mount_id.clone(), m.resolve_path(&base_dir)?)))
        .collect::<Result<HashMap<_, _>, AppError>>()?;

    // Step 2: 旧 mount_id_map を snapshot（短時間 lock、即解放）
    let old_entries: HashMap<String, PathBuf> = {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let reg = state
            .node_registry
            .lock()
            .expect("NodeRegistry Mutex poisoned");
        reg.mount_id_map().clone()
    };

    // Step 3: 差分を算出
    let old_ids: HashSet<&str> = old_entries.keys().map(String::as_str).collect();
    let new_ids: HashSet<&str> = new_entries.keys().map(String::as_str).collect();
    let removed_ids: Vec<String> = old_ids
        .difference(&new_ids)
        .map(|s| (*s).to_string())
        .collect();
    let ignored_additions: Vec<String> = new_ids
        .difference(&old_ids)
        .map(|s| (*s).to_string())
        .collect();

    if !ignored_additions.is_empty() {
        tracing::warn!(
            mount_ids = ?ignored_additions,
            "mount 追加は hot reload 非対応（docker-compose 再起動が必要、skip）"
        );
    }

    // Step 4: 削除 mount の stale cleanup を spawn_blocking 内で実行
    //   SQLite I/O は async runtime を塞がないよう blocking 境界に隔離する
    let cleanup_ok = if removed_ids.is_empty() {
        true
    } else {
        let indexer = Arc::clone(&state.indexer);
        let dir_index = Arc::clone(&state.dir_index);
        let ids_for_task = removed_ids.clone();
        // `shutdown_token` を clone して `spawn_blocking` 内で参照可能にする
        let cancel_token = state.shutdown_token.clone();
        match tokio::task::spawn_blocking(move || {
            let cancelled_fn = || cancel_token.is_cancelled();
            perform_full_stale_cleanup(&ids_for_task, &indexer, &dir_index, &cancelled_fn)
        })
        .await
        {
            Ok(ok) => ok,
            Err(e) => {
                tracing::error!("stale cleanup task panic: {e}");
                false
            }
        }
    };

    // cleanup 直後の check: cleanup 中に cancel された場合は 503 に寄せる
    //   （cleanup が部分成功でも成功扱いを避ける、shutdown を観測から明示）
    if state.shutdown_token.is_cancelled() {
        return Err(AppError::ShutdownInProgress(
            "shutdown 中に mounts reload の stale cleanup が中断された".to_string(),
        ));
    }

    // Step 5: NodeRegistry から削除 mount を除去（短時間 lock）
    {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let mut reg = state
            .node_registry
            .lock()
            .expect("NodeRegistry Mutex poisoned");
        for id in &removed_ids {
            reg.remove_mount(id);
        }
    }

    // Step 6: PathSecurity の roots を新構成で差し替え
    //   NodeRegistry / FileWatcher が保持する Arc<PathSecurity> は同一 Arc のまま、
    //   内部 RwLock 越しに新 roots が反映される
    let new_roots: Vec<PathBuf> = new_entries.values().cloned().collect();
    if new_roots.is_empty() {
        // 空 roots は PathSecurity::replace_roots が拒否する。新 config が
        // 全マウント削除だった場合はここで早期警告して pathsecurity は触らない
        tracing::warn!("新 mounts.json のマウント数が 0、PathSecurity は既存 roots を維持");
    } else {
        state.path_security.replace_roots(new_roots.clone())?;
    }

    // Step 7: NodeRegistry 内の root_entries キャッシュを再構築
    {
        #[allow(
            clippy::expect_used,
            reason = "Mutex poison は致命的エラー、パニックが適切"
        )]
        let mut reg = state
            .node_registry
            .lock()
            .expect("NodeRegistry Mutex poisoned");
        reg.rebuild_root_entries_cache();
    }

    // watcher 再起動前の check: shutdown 中なら新 watcher を起動せず 503 を返す
    //   （late-install 抑止、main の drain_long_tasks と競合しない）
    if state.shutdown_token.is_cancelled() {
        return Err(AppError::ShutdownInProgress(
            "shutdown 中のため FileWatcher 再起動を抑止".to_string(),
        ));
    }

    // Step 8: FileWatcher を新構成で再起動
    //   旧 watcher の stop → 新 watcher::new + start → slot に replace
    if !new_roots.is_empty() {
        let watcher_mounts: Vec<(String, PathBuf)> = new_entries
            .iter()
            .map(|(id, p)| (id.clone(), p.clone()))
            .collect();

        let indexer = Arc::clone(&state.indexer);
        let path_security = Arc::clone(&state.path_security);
        let dir_index = Arc::clone(&state.dir_index);
        let rebuild_guard = Arc::clone(&state.rebuild_guard);

        let old_watcher = {
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut slot = state
                .file_watcher
                .lock()
                .expect("file_watcher Mutex poisoned");
            slot.take()
        };
        if let Some(fw) = old_watcher {
            fw.stop();
        }

        let new_watcher = FileWatcher::new(
            indexer,
            path_security,
            dir_index,
            watcher_mounts,
            rebuild_guard,
        );
        if let Err(e) = new_watcher.start() {
            tracing::error!("hot reload 後の FileWatcher 起動失敗: {e}");
            // 起動失敗でも slot は None のまま（次回 reload で再試行可能）
        } else {
            #[allow(
                clippy::expect_used,
                reason = "Mutex poison は致命的エラー、パニックが適切"
            )]
            let mut slot = state
                .file_watcher
                .lock()
                .expect("file_watcher Mutex poisoned");
            *slot = Some(new_watcher);
        }
    }

    tracing::info!(
        removed = ?removed_ids,
        ignored_additions = ?ignored_additions,
        cleanup_ok,
        "mount hot reload 完了"
    );

    Ok(MountReloadResult {
        removed: removed_ids,
        ignored_additions,
        cleanup_ok,
    })
}

#[cfg(test)]
#[allow(
    non_snake_case,
    reason = "日本語テスト名で振る舞いを記述する規約 (07_testing.md)"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use tempfile::TempDir;

    use super::*;
    use crate::config::Settings;
    use crate::services::archive::ArchiveService;
    use crate::services::dir_index::DirIndex;
    use crate::services::indexer::Indexer;
    use crate::services::mount_config::{MountConfig, MountPoint};
    use crate::services::node_registry::NodeRegistry;
    use crate::services::path_security::PathSecurity;
    use crate::services::rebuild_guard::RebuildGuard;
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;
    use crate::state::AppState;

    const MOUNT_A: &str = "aaaaaaaaaaaaaaaa";
    const MOUNT_B: &str = "bbbbbbbbbbbbbbbb";

    struct Env {
        state: Arc<AppState>,
        #[allow(dead_code, reason = "TempDir を drop させないため保持")]
        root: TempDir,
        #[allow(dead_code, reason = "TempDir を drop させないため保持")]
        db_dir: TempDir,
    }

    fn setup_two_mounts() -> Env {
        let root = TempDir::new().unwrap();
        let base = std::fs::canonicalize(root.path()).unwrap();
        std::fs::create_dir_all(base.join("a")).unwrap();
        std::fs::create_dir_all(base.join("b")).unwrap();

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            base.to_string_lossy().into_owned(),
        )]))
        .unwrap();

        let ps = Arc::new(PathSecurity::new(vec![base.join("a"), base.join("b")], false).unwrap());
        let mut registry = NodeRegistry::new(Arc::clone(&ps), 100_000, HashMap::new());
        let mut mount_id_map = HashMap::new();
        mount_id_map.insert(MOUNT_A.to_string(), base.join("a"));
        mount_id_map.insert(MOUNT_B.to_string(), base.join("b"));
        registry.set_mount_id_map(mount_id_map);

        let archive_service = Arc::new(ArchiveService::new(&settings));
        let temp_file_cache =
            Arc::new(TempFileCache::new(TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap());
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));

        let db_dir = TempDir::new().unwrap();
        let indexer = Arc::new(Indexer::new(
            db_dir.path().join("index.db").to_str().unwrap(),
        ));
        indexer.init_db().unwrap();
        let dir_index = Arc::new(DirIndex::new(
            db_dir.path().join("dir.db").to_str().unwrap(),
        ));
        dir_index.init_db().unwrap();
        // 両 mount を fingerprint に保存（後で削除判定に使う）
        indexer.save_mount_fingerprint(&[MOUNT_A, MOUNT_B]).unwrap();

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
            registry_populate_stats: Arc::new(
                crate::services::node_registry::PopulateStats::default(),
            ),
            last_scan_report: Arc::new(std::sync::RwLock::new(None)),
            rebuild_guard: Arc::new(RebuildGuard::new()),
            file_watcher: Arc::new(Mutex::new(None)),
            path_security: ps,
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            rebuild_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            rebuild_task: Arc::new(std::sync::Mutex::new(None)),
        });

        Env {
            state,
            root,
            db_dir,
        }
    }

    fn config_with_mounts(env: &Env, slugs: &[(&str, &str)]) -> MountConfig {
        let mounts = slugs
            .iter()
            .map(|(id, slug)| MountPoint {
                mount_id: (*id).to_string(),
                name: (*id).to_string(),
                slug: (*slug).to_string(),
                host_path: String::new(),
            })
            .collect();
        // TempDir の path() を settings で使っているため canonicalize 済みに揃える
        let _ = env.state.settings.mount_base_dir.clone();
        MountConfig { mounts }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reload_mountsは削除mountをremovedに載せる() {
        let env = setup_two_mounts();
        // 新 config は MOUNT_A のみ（MOUNT_B が削除される）
        let new_config = config_with_mounts(&env, &[(MOUNT_A, "a")]);
        let result = reload_mounts(Arc::clone(&env.state), new_config)
            .await
            .expect("reload_mounts が成功するはず");
        assert_eq!(result.removed, vec![MOUNT_B.to_string()]);
        assert!(result.ignored_additions.is_empty());
        assert!(result.cleanup_ok);
        // mount_id_map からも MOUNT_B が消える
        let reg = env.state.node_registry.lock().unwrap();
        assert!(!reg.mount_id_map().contains_key(MOUNT_B));
        assert!(reg.mount_id_map().contains_key(MOUNT_A));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reload_mountsは追加mountをignored_additionsに載せてスキップする() {
        let env = setup_two_mounts();
        std::fs::create_dir_all(env.state.settings.mount_base_dir.join("c")).unwrap();
        // 新 config は MOUNT_A, MOUNT_B, NEW（追加）
        const NEW: &str = "cccccccccccccccc";
        let new_config = config_with_mounts(&env, &[(MOUNT_A, "a"), (MOUNT_B, "b"), (NEW, "c")]);
        let result = reload_mounts(Arc::clone(&env.state), new_config)
            .await
            .expect("reload_mounts が成功するはず");
        assert!(result.removed.is_empty());
        assert_eq!(result.ignored_additions, vec![NEW.to_string()]);
        // 追加 mount は NodeRegistry に反映されない
        let reg = env.state.node_registry.lock().unwrap();
        assert!(!reg.mount_id_map().contains_key(NEW));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reload_mountsはPathSecurityを新rootsに差し替える() {
        let env = setup_two_mounts();
        let base = env.state.settings.mount_base_dir.clone();
        // 削除前は両 root が validate 可能
        assert!(env.state.path_security.validate(&base.join("a")).is_ok());
        assert!(env.state.path_security.validate(&base.join("b")).is_ok());

        // MOUNT_B のみ残す (MOUNT_A 削除)
        let new_config = config_with_mounts(&env, &[(MOUNT_B, "b")]);
        reload_mounts(Arc::clone(&env.state), new_config)
            .await
            .expect("reload_mounts が成功するはず");

        // MOUNT_A (/a) は PathSecurity から外れた
        assert!(env.state.path_security.validate(&base.join("a")).is_err());
        assert!(env.state.path_security.validate(&base.join("b")).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reload_mountsはshutdown_token_cancel時にShutdownInProgressを返す() {
        let env = setup_two_mounts();
        // shutdown_token を先に cancel
        env.state.shutdown_token.cancel();

        let new_config = config_with_mounts(&env, &[(MOUNT_A, "a")]);
        let result = reload_mounts(Arc::clone(&env.state), new_config).await;
        match result {
            Err(AppError::ShutdownInProgress(msg)) => {
                assert!(msg.contains("shutdown"), "shutdown メッセージを含むべき");
            }
            other => panic!("ShutdownInProgress を期待: {other:?}"),
        }
        // mount_id_map は変更されない（cancel で早期 return）
        let reg = env.state.node_registry.lock().unwrap();
        assert!(reg.mount_id_map().contains_key(MOUNT_A));
        assert!(reg.mount_id_map().contains_key(MOUNT_B));
    }
}
