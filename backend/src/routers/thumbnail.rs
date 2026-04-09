//! サムネイル API
//!
//! - `GET /api/thumbnail/{node_id}` — 単一サムネイル (JPEG)
//! - `POST /api/thumbnails/batch` — バッチサムネイル (base64 JSON)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::services::extensions::{IMAGE_EXTENSIONS, PDF_EXTENSIONS, VIDEO_EXTENSIONS};
use crate::state::AppState;

/// サムネイル `ETag` を計算する
///
/// `MD5("thumb:{mtime_ns}:{node_id}")`
fn compute_thumb_etag(mtime_ns: u128, node_id: &str) -> String {
    let raw = format!("thumb:{mtime_ns}:{node_id}");
    let digest = Md5::digest(raw.as_bytes());
    format!("\"{digest:x}\"")
}

/// ファイルの `mtime_ns` を取得する
fn get_mtime_ns(path: &std::path::Path) -> Result<u128, AppError> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::NodeNotFound {
        node_id: String::new(),
    })?;
    Ok(meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos()))
}

/// ファイルの `mtime_ns` を取得する（ディレクトリは拒否、metadata 1 回のみ）
///
/// `is_dir()` + `get_mtime_ns()` の 2 回 stat を統合。
/// 類似: `thumbnail_warmer.rs` の `mtime_ns_of`（`std::io::Error` 版、ディレクトリ判定なし）
fn file_mtime_ns(path: &std::path::Path, node_id: &str) -> Result<u128, AppError> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::NodeNotFound {
        node_id: node_id.to_string(),
    })?;
    if meta.is_dir() {
        return Err(AppError::NotSupported(
            "ディレクトリのサムネイルは非対応です".to_string(),
        ));
    }
    Ok(meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos()))
}

/// 拡張子を小文字で取得する
fn ext_lower(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .map_or(String::new(), |e| {
            format!(".{}", e.to_string_lossy().to_lowercase())
        })
}

#[derive(Deserialize)]
pub(crate) struct ThumbnailQuery {
    /// バージョンパラメータ (キャッシュバスティング)
    v: Option<String>,
}

/// `GET /api/thumbnail/{node_id}` — 単一サムネイルを返す
///
/// - 画像: `image` + `fast_image_resize` で 300px JPEG
/// - アーカイブ: 先頭画像エントリのサムネイル
/// - PDF: `pdftoppm` で先頭ページ
/// - 動画: `FFmpeg` フレーム抽出
/// - `ETag` + `Cache-Control` 付き
pub(crate) async fn serve_thumbnail(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Query(query): Query<ThumbnailQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // サムネイル生成は CPU バウンド → spawn_blocking
    let result = {
        let state = Arc::clone(&state);
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(move || generate_thumbnail_bytes(&state, &node_id))
            .await
            .map_err(|e| AppError::InvalidImage(format!("タスク実行失敗: {e}")))?
    }?;

    // If-None-Match チェック
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(val) = if_none_match.to_str() {
            if val == result.etag {
                return Ok(StatusCode::NOT_MODIFIED.into_response());
            }
        }
    }

    // Cache-Control ヘッダ
    let cache_control = if query.v.is_some() {
        "public, max-age=31536000, immutable"
    } else {
        "private, max-age=3600"
    };

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    if let Ok(etag_val) = HeaderValue::from_str(&result.etag) {
        resp_headers.insert(header::ETAG, etag_val);
    }
    if let Ok(cc_val) = HeaderValue::from_str(cache_control) {
        resp_headers.insert(header::CACHE_CONTROL, cc_val);
    }

    Ok((resp_headers, result.data).into_response())
}

/// サムネイル生成結果
struct ThumbnailResult {
    data: Vec<u8>,
    etag: String,
}

/// サムネイル生成のコアロジック (sync、`spawn_blocking` 内から呼ぶ)
///
/// `node_id` の種類に応じて適切な生成パスを選択する:
/// 1. アーカイブエントリ → 抽出 → リサイズ
/// 2. 通常ファイル:
///    - ディレクトリ → `NOT_SUPPORTED`
///    - アーカイブファイル → 先頭画像エントリ → リサイズ
///    - PDF → `pdftoppm` → リサイズ
///    - 動画 → `FFmpeg` フレーム抽出 → リサイズ
///    - 画像 → リサイズ
///    - その他 → `NOT_SUPPORTED`
fn generate_thumbnail_bytes(state: &AppState, node_id: &str) -> Result<ThumbnailResult, AppError> {
    let thumb_svc = &state.thumbnail_service;
    let video_conv = &state.video_converter;

    // アーカイブエントリかチェック
    let entry = {
        let mut registry = state
            .node_registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.resolve_archive_entry(node_id)
    };

    if let Some((archive_path, entry_name)) = entry {
        let mtime_ns = get_mtime_ns(&archive_path)?;
        let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
        let etag = compute_thumb_etag(mtime_ns, node_id);
        // キャッシュヒット時は外部プロセスをスキップ
        if let Some(cached) = thumb_svc.try_read_cached(&cache_key) {
            return Ok(ThumbnailResult { data: cached, etag });
        }
        let data = state
            .archive_service
            .extract_entry(&archive_path, &entry_name)?;
        let thumb = thumb_svc.generate_from_bytes(&data, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 通常パス解決
    let resolved = {
        let registry = state
            .node_registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.resolve(node_id)?.to_path_buf()
    };

    // ディレクトリチェック + mtime 取得 (metadata 1 回)
    let mtime_ns = file_mtime_ns(&resolved, node_id)?;
    let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
    let etag = compute_thumb_etag(mtime_ns, node_id);

    let ext = ext_lower(&resolved.to_string_lossy());

    // アーカイブファイル自体 → 先頭画像エントリのサムネイル
    if state.archive_service.is_supported(&resolved) {
        // キャッシュヒット時は list_entries + extract_entry をスキップ
        if let Some(cached) = thumb_svc.try_read_cached(&cache_key) {
            return Ok(ThumbnailResult { data: cached, etag });
        }
        let entry = state
            .archive_service
            .first_image_entry(&resolved)?
            .ok_or_else(|| AppError::NoImage("アーカイブ内に画像が見つかりません".to_string()))?;
        let data = state
            .archive_service
            .extract_entry(&resolved, &entry.name)?;
        let thumb = thumb_svc.generate_from_bytes(&data, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // PDF
    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_pdf_thumbnail(
            &resolved,
            &cache_key,
            state.settings.video_thumb_timeout,
        )?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 動画
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        // キャッシュヒット時は ffmpeg をスキップ
        if let Some(cached) = thumb_svc.try_read_cached(&cache_key) {
            return Ok(ThumbnailResult { data: cached, etag });
        }
        let frame = video_conv.extract_frame(&resolved).ok_or_else(|| {
            AppError::FrameExtractFailed("動画のフレーム抽出に失敗しました".to_string())
        })?;
        let thumb = thumb_svc.generate_from_bytes(&frame, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // 画像
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_from_path(&resolved, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    // その他
    Err(AppError::NotSupported(
        "サムネイル非対応のファイル形式です".to_string(),
    ))
}

/// 事前解決済みパスからサムネイルを生成する (batch API 専用、registry lock 不要)
fn generate_thumbnail_from_resolved(
    state: &AppState,
    node_id: &str,
    resolved: &std::path::Path,
) -> Result<ThumbnailResult, AppError> {
    let thumb_svc = &state.thumbnail_service;
    let video_conv = &state.video_converter;

    // ディレクトリチェック + mtime 取得 (metadata 1 回)
    let mtime_ns = file_mtime_ns(resolved, node_id)?;
    let cache_key = thumb_svc.make_cache_key(node_id, mtime_ns);
    let etag = compute_thumb_etag(mtime_ns, node_id);
    let ext = ext_lower(&resolved.to_string_lossy());

    // アーカイブファイル自体 → 先頭画像エントリのサムネイル
    if state.archive_service.is_supported(resolved) {
        if let Some(cached) = thumb_svc.try_read_cached(&cache_key) {
            return Ok(ThumbnailResult { data: cached, etag });
        }
        let entry = state
            .archive_service
            .first_image_entry(resolved)?
            .ok_or_else(|| AppError::NoImage("アーカイブ内に画像が見つかりません".to_string()))?;
        let data = state.archive_service.extract_entry(resolved, &entry.name)?;
        let thumb = thumb_svc.generate_from_bytes(&data, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_pdf_thumbnail(
            resolved,
            &cache_key,
            state.settings.video_thumb_timeout,
        )?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        if let Some(cached) = thumb_svc.try_read_cached(&cache_key) {
            return Ok(ThumbnailResult { data: cached, etag });
        }
        let frame = video_conv.extract_frame(resolved).ok_or_else(|| {
            AppError::FrameExtractFailed("動画のフレーム抽出に失敗しました".to_string())
        })?;
        let thumb = thumb_svc.generate_from_bytes(&frame, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        let thumb = thumb_svc.generate_from_path(resolved, &cache_key)?;
        return Ok(ThumbnailResult { data: thumb, etag });
    }

    Err(AppError::NotSupported(
        "サムネイル非対応のファイル形式です".to_string(),
    ))
}

// --- バッチ API ---

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
/// - 最大 50 件、重複排除
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

    // 50 件上限 + 重複排除 (順序保持)
    let mut seen = std::collections::HashSet::new();
    let unique_ids: Vec<String> = body
        .node_ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .take(50)
        .collect();

    // アーカイブエントリをグループ化 + 通常ファイルのパス解決 (registry lock 1 回のみ)
    let (archive_groups, regular_entries, unresolved_ids) = classify_node_ids(&state, &unique_ids);

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

    // 非アーカイブエントリの個別処理タスク (事前解決済みパスを使用、registry lock 不要)
    let mut regular_handles = Vec::with_capacity(regular_entries.len());

    for (nid, resolved) in &regular_entries {
        let state = Arc::clone(&state);
        let nid = nid.clone();
        let resolved = resolved.clone();
        let sem = Arc::clone(&state.thumb_semaphore);

        regular_handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let result = tokio::task::spawn_blocking(move || {
                generate_thumbnail_from_resolved(&state, &nid, &resolved)
            })
            .await;

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
        }));
    }

    // 結果を統合
    let mut thumbnails = HashMap::with_capacity(unique_ids.len());

    // アーカイブグループ結果
    for handle in archive_handles {
        if let Ok(group_map) = handle.await {
            thumbnails.extend(group_map);
        }
    }

    // 非アーカイブ結果
    for ((nid, _), handle) in regular_entries.into_iter().zip(regular_handles) {
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
