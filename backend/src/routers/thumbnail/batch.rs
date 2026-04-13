//! バッチサムネイル API
//!
//! `POST /api/thumbnails/batch` — 複数 `node_id` のサムネイルを一括生成

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::{compute_thumb_etag, file_meta, generate_thumbnail_with_mtime, get_mtime_ns};
use crate::errors::AppError;
use crate::state::AppState;

#[derive(Deserialize)]
pub(crate) struct BatchRequest {
    node_ids: Vec<String>,
}

#[derive(Serialize)]
struct BatchThumbnailEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

#[derive(Serialize)]
struct BatchResponse {
    thumbnails: HashMap<String, BatchThumbnailEntry>,
}

/// アーカイブグループ: アーカイブパス → `[(node_id, entry_name)]`
type ArchiveGroups = HashMap<std::path::PathBuf, Vec<(String, String)>>;

/// `classify_node_ids` の戻り値: (アーカイブグループ, 通常エントリ, resolve 失敗 ID)
type ClassifiedNodes = (
    ArchiveGroups,
    Vec<(String, std::path::PathBuf)>,
    Vec<String>,
);

/// アーカイブエントリをグループ化し、通常ファイルはパス解決も同時に行う (registry lock 1 回)
///
/// resolve 失敗した `node_id` は `unresolved_ids` として返す。
fn classify_node_ids(state: &AppState, node_ids: &[String]) -> ClassifiedNodes {
    let mut registry = state
        .node_registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut archive_groups: HashMap<std::path::PathBuf, Vec<(String, String)>> = HashMap::new();
    let mut regular_entries = Vec::new();
    let mut unresolved_ids = Vec::new();

    for nid in node_ids {
        if let Some((archive_path, entry_name)) = registry.resolve_archive_entry(nid) {
            archive_groups
                .entry(archive_path)
                .or_default()
                .push((nid.clone(), entry_name));
        } else if let Ok(path) = registry.resolve(nid) {
            regular_entries.push((nid.clone(), path.to_path_buf()));
        } else {
            unresolved_ids.push(nid.clone());
        }
    }

    (archive_groups, regular_entries, unresolved_ids)
}

/// プレチェック結果: キャッシュ済み / 生成必要 / スキップ（エラー）
enum PreCheckResult {
    /// キャッシュヒット — セマフォ不要で即座に返却可能
    Cached { data: Vec<u8>, etag: String },
    /// キャッシュミス — セマフォ取得後に生成が必要
    NeedsGeneration {
        resolved: std::path::PathBuf,
        mtime_ns: u128,
        file_size: u64,
    },
    /// ディレクトリ / stat 失敗 — エラーエントリとして即座に返却
    Skipped(BatchThumbnailEntry),
}

/// 通常エントリのキャッシュプレチェック (`spawn_blocking` 内で実行)
///
/// 各エントリに対して `metadata()` 1 回でディレクトリ判定 + mtime 取得 → キャッシュ確認。
/// キャッシュ済みエントリはセマフォを経由せず即座に返却できる。
fn pre_check_regular_entries(
    state: &AppState,
    entries: &[(String, std::path::PathBuf)],
) -> Vec<(String, PreCheckResult)> {
    let thumb_svc = &state.thumbnail_service;

    entries
        .iter()
        .map(|(nid, resolved)| {
            let result = match file_meta(resolved, nid) {
                Ok((mtime_ns, file_size)) => {
                    let cache_key = thumb_svc.make_cache_key(nid, mtime_ns);
                    let etag = compute_thumb_etag(mtime_ns, nid);
                    if let Some(data) = thumb_svc.try_read_cached(&cache_key) {
                        PreCheckResult::Cached { data, etag }
                    } else {
                        PreCheckResult::NeedsGeneration {
                            resolved: resolved.clone(),
                            mtime_ns,
                            file_size,
                        }
                    }
                }
                Err(app_err) => {
                    let (code, msg) = error_to_code_message(&app_err);
                    PreCheckResult::Skipped(BatchThumbnailEntry {
                        data: None,
                        etag: None,
                        error: Some(msg),
                        code: Some(code),
                    })
                }
            };
            (nid.clone(), result)
        })
        .collect()
}

/// 同一アーカイブの複数エントリを一括展開してサムネイル生成する
///
/// アーカイブオープン失敗時は該当グループ全エントリに `INVALID_ARCHIVE` エラーを返す。
fn generate_archive_group_thumbnails(
    state: &AppState,
    archive_path: &std::path::Path,
    entries: &[(String, String)],
) -> HashMap<String, BatchThumbnailEntry> {
    use base64::Engine;

    let mut results = HashMap::with_capacity(entries.len());

    let Ok(mtime_ns) = get_mtime_ns(archive_path) else {
        // アーカイブファイルが読めない → 全エントリにエラー
        for (nid, _) in entries {
            results.insert(
                nid.clone(),
                BatchThumbnailEntry {
                    data: None,
                    etag: None,
                    error: Some("アーカイブファイルが見つかりません".to_string()),
                    code: Some("INVALID_ARCHIVE".to_string()),
                },
            );
        }
        return results;
    };

    // 一括抽出
    let entry_names: Vec<String> = entries.iter().map(|(_, name)| name.clone()).collect();
    let batch_result = state
        .archive_service
        .extract_entries_batch(archive_path, &entry_names);

    let extracted = match batch_result {
        Ok(data) => data,
        Err(err) => {
            // 一括抽出失敗 → 全エントリにエラー
            let (code, msg) = error_to_code_message(&err);
            for (nid, _) in entries {
                results.insert(
                    nid.clone(),
                    BatchThumbnailEntry {
                        data: None,
                        etag: None,
                        error: Some(msg.clone()),
                        code: Some(code.clone()),
                    },
                );
            }
            return results;
        }
    };

    // 個別エントリのサムネイル生成
    let thumb_svc = &state.thumbnail_service;
    for (nid, entry_name) in entries {
        let etag = compute_thumb_etag(mtime_ns, nid);
        let cache_key = thumb_svc.make_cache_key(nid, mtime_ns);

        let entry = if let Some(data) = extracted.get(entry_name) {
            match thumb_svc.generate_from_bytes(data, &cache_key) {
                Ok(thumb) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&thumb);
                    BatchThumbnailEntry {
                        data: Some(b64),
                        etag: Some(etag),
                        error: None,
                        code: None,
                    }
                }
                Err(err) => {
                    let (code, msg) = error_to_code_message(&err);
                    BatchThumbnailEntry {
                        data: None,
                        etag: None,
                        error: Some(msg),
                        code: Some(code),
                    }
                }
            }
        } else {
            BatchThumbnailEntry {
                data: None,
                etag: None,
                error: Some(format!("エントリが見つかりません: {entry_name}")),
                code: Some("NOT_FOUND".to_string()),
            }
        };

        results.insert(nid.clone(), entry);
    }

    results
}

/// `POST /api/thumbnails/batch` — バッチサムネイルを返す
///
/// - 最大 100 件、重複排除
/// - 同一アーカイブのエントリをグループ化して一括処理
/// - 全体ステータスは常に 200
#[allow(
    clippy::too_many_lines,
    reason = "アーカイブ/通常の分岐 + タスク統合で行数が増加"
)]
pub(crate) async fn serve_thumbnails_batch(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BatchRequest>,
) -> Response {
    use base64::Engine;

    // 計測: 全体経過時間
    let started = std::time::Instant::now();

    // 100 件上限 + 重複排除 (順序保持)
    // browse API の page size (100) と揃える。セマフォで並行度制限するため過負荷リスクなし
    let mut seen = std::collections::HashSet::new();
    let unique_ids: Vec<String> = body
        .node_ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .take(100)
        .collect();
    let request_count = unique_ids.len();

    // アーカイブエントリをグループ化 + 通常ファイルのパス解決 (registry lock 1 回のみ)
    let (archive_groups, regular_entries, unresolved_ids) = classify_node_ids(&state, &unique_ids);
    let archive_group_count = archive_groups.len();

    // アーカイブグループの一括処理タスク (AppState セマフォで並行度制限)
    let mut archive_handles = Vec::new();
    for (arc_path, entries) in archive_groups {
        let state = Arc::clone(&state);
        let sem = Arc::clone(&state.archive_thumb_semaphore);
        archive_handles.push(tokio::spawn(async move {
            let Ok(_permit) = sem.acquire().await else {
                return HashMap::new();
            };
            tokio::task::spawn_blocking(move || {
                generate_archive_group_thumbnails(&state, &arc_path, &entries)
            })
            .await
            .unwrap_or_default()
        }));
    }

    // --- Phase 1: キャッシュプレチェック (spawn_blocking、セマフォ不要) ---
    // 各エントリに metadata() 1 回 → キャッシュ確認し、Cached / NeedsGeneration / Skipped に分類
    let pre_check_state = Arc::clone(&state);
    let pre_checked = tokio::task::spawn_blocking(move || {
        pre_check_regular_entries(&pre_check_state, &regular_entries)
    })
    .await
    .unwrap_or_default();

    // 計測: cache_state 集計 (regular エントリのみ。アーカイブグループは別フェーズ)
    let mut hit = 0_usize;
    let mut miss = 0_usize;
    let mut skipped = 0_usize;
    for (_, result) in &pre_checked {
        match result {
            PreCheckResult::Cached { .. } => hit += 1,
            PreCheckResult::NeedsGeneration { .. } => miss += 1,
            PreCheckResult::Skipped(_) => skipped += 1,
        }
    }

    // --- Phase 2: キャッシュミスのみセマフォ + 生成タスクを起動 ---
    let mut regular_handles: Vec<(String, tokio::task::JoinHandle<BatchThumbnailEntry>)> =
        Vec::new();
    let mut thumbnails = HashMap::with_capacity(unique_ids.len());

    for (nid, result) in pre_checked {
        match result {
            PreCheckResult::Cached { data, etag } => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                thumbnails.insert(
                    nid,
                    BatchThumbnailEntry {
                        data: Some(b64),
                        etag: Some(etag),
                        error: None,
                        code: None,
                    },
                );
            }
            PreCheckResult::Skipped(entry) => {
                thumbnails.insert(nid, entry);
            }
            PreCheckResult::NeedsGeneration {
                resolved,
                mtime_ns,
                file_size,
            } => {
                let state = Arc::clone(&state);
                let nid_clone = nid.clone();
                let sem = Arc::clone(&state.thumb_semaphore);

                let handle = tokio::spawn(async move {
                    let _permit = sem.acquire().await;
                    let gen_started = std::time::Instant::now();
                    let nid_for_log = nid_clone.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        generate_thumbnail_with_mtime(
                            &state, &nid_clone, &resolved, mtime_ns, file_size,
                        )
                    })
                    .await;
                    tracing::info!(
                        node_id = &nid_for_log[..nid_for_log.len().min(8)],
                        source = "batch",
                        elapsed_us =
                            u64::try_from(gen_started.elapsed().as_micros()).unwrap_or(u64::MAX),
                        "thumbnail.generated"
                    );

                    match result {
                        Ok(Ok(thumb)) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&thumb.data);
                            BatchThumbnailEntry {
                                data: Some(b64),
                                etag: Some(thumb.etag),
                                error: None,
                                code: None,
                            }
                        }
                        Ok(Err(app_err)) => {
                            let (code, msg) = error_to_code_message(&app_err);
                            BatchThumbnailEntry {
                                data: None,
                                etag: None,
                                error: Some(msg),
                                code: Some(code),
                            }
                        }
                        Err(_join_err) => BatchThumbnailEntry {
                            data: None,
                            etag: None,
                            error: Some("タスク実行エラー".to_string()),
                            code: Some("INTERNAL_ERROR".to_string()),
                        },
                    }
                });
                regular_handles.push((nid, handle));
            }
        }
    }

    // アーカイブグループ結果
    for handle in archive_handles {
        if let Ok(group_map) = handle.await {
            thumbnails.extend(group_map);
        }
    }

    // キャッシュミス生成結果
    for (nid, handle) in regular_handles {
        let entry = handle.await.unwrap_or(BatchThumbnailEntry {
            data: None,
            etag: None,
            error: Some("タスク実行エラー".to_string()),
            code: Some("INTERNAL_ERROR".to_string()),
        });
        thumbnails.insert(nid, entry);
    }

    // resolve 失敗分をエラーエントリとして追加
    for nid in unresolved_ids {
        thumbnails.insert(
            nid,
            BatchThumbnailEntry {
                data: None,
                etag: None,
                error: Some("ノードが見つかりません".to_string()),
                code: Some("NOT_FOUND".to_string()),
            },
        );
    }

    // 計測ログ: cache_state は regular エントリの hit/miss/skipped 比率から判定
    // - empty: regular 0 件 (アーカイブのみ or 空リクエスト)
    // - all_skipped: 全件スキップ (ディレクトリ等の非対応エントリ)
    // - all_hit: 全件キャッシュヒット
    // - all_miss: 全件キャッシュミス
    // - partial_hit: 上記以外 (ヒット・ミス・スキップが混在)
    let regular_total = hit + miss + skipped;
    let cache_state = if regular_total == 0 {
        "empty"
    } else if skipped == regular_total {
        "all_skipped"
    } else if hit == regular_total {
        "all_hit"
    } else if miss == regular_total {
        "all_miss"
    } else {
        "partial_hit"
    };
    tracing::info!(
        request_count,
        archive_groups = archive_group_count,
        cache_state,
        hit,
        miss,
        skipped,
        elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        "thumbnail.batch completed"
    );

    Json(BatchResponse { thumbnails }).into_response()
}

/// `AppError` からエラーコードとメッセージを抽出する
fn error_to_code_message(err: &AppError) -> (String, String) {
    let code = match err {
        AppError::NodeNotFound { .. } => "NOT_FOUND",
        AppError::NotSupported(_) => "NOT_SUPPORTED",
        AppError::InvalidImage(_) => "INVALID_IMAGE",
        AppError::NoImage(_) => "NO_IMAGE",
        AppError::FrameExtractFailed(_) => "FRAME_EXTRACT_FAILED",
        AppError::InvalidArchive(_) => "INVALID_ARCHIVE",
        _ => "INTERNAL_ERROR",
    };
    (code.to_string(), err.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use axum::{Router, body};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Settings;
    use crate::services::dir_index::DirIndex;
    use crate::services::node_registry::NodeRegistry;
    use crate::services::path_security::PathSecurity;
    use crate::services::temp_file_cache::TempFileCache;
    use crate::services::thumbnail_service::ThumbnailService;
    use crate::services::thumbnail_warmer::ThumbnailWarmer;
    use crate::services::video_converter::VideoConverter;

    fn register_file(state: &Arc<AppState>, path: &std::path::Path) -> String {
        let mut reg = state.node_registry.lock().unwrap();
        reg.register(path).unwrap()
    }

    fn batch_setup() -> (Router, Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            root.to_string_lossy().into_owned(),
        )]))
        .unwrap();

        let ps = Arc::new(PathSecurity::new(vec![root], false).unwrap());
        let registry = NodeRegistry::new(ps, 100_000, HashMap::new());
        let archive_service = Arc::new(crate::services::archive::ArchiveService::new(&settings));
        let temp_file_cache = Arc::new(
            TempFileCache::new(tempfile::TempDir::new().unwrap().keep(), 10 * 1024 * 1024).unwrap(),
        );
        let thumbnail_service = Arc::new(ThumbnailService::new(Arc::clone(&temp_file_cache)));
        let video_converter =
            Arc::new(VideoConverter::new(Arc::clone(&temp_file_cache), &settings));
        let thumbnail_warmer = Arc::new(ThumbnailWarmer::new(4));
        let index_db = tempfile::NamedTempFile::new().unwrap();
        let indexer = Arc::new(crate::services::indexer::Indexer::new(
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
        });

        let app = Router::new()
            .route("/api/thumbnails/batch", post(serve_thumbnails_batch))
            .with_state(Arc::clone(&app_state));

        (app, app_state, dir)
    }

    #[tokio::test]
    async fn resolve成功と失敗が混在するバッチで両方レスポンスに含まれる() {
        let (app, state, dir) = batch_setup();

        // 登録済みファイル (サムネイル生成は成否問わず、レスポンスに含まれることを検証)
        let img = dir.path().join("test.jpg");
        std::fs::write(&img, b"fake-jpeg-data").unwrap();
        let valid_id = register_file(&state, &img);

        // 未登録の偽 node_id
        let fake_id = "nonexistent_node_id".to_string();

        let payload = serde_json::json!({ "node_ids": [valid_id, fake_id] });
        let resp = app
            .oneshot(
                Request::post("/api/thumbnails/batch")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let thumbs = json["thumbnails"].as_object().unwrap();

        // 登録済みファイル: レスポンスに含まれること (エラーでも可)
        assert!(
            thumbs.contains_key(&valid_id),
            "登録済み node_id がレスポンスに含まれない"
        );

        // 未登録 node_id: NOT_FOUND エラーとしてレスポンスに含まれること
        let fake_entry = thumbs
            .get(&fake_id)
            .expect("未登録 node_id がレスポンスに含まれない");
        assert_eq!(fake_entry["code"].as_str().unwrap(), "NOT_FOUND");
        assert!(fake_entry["error"].is_string());
    }
}
