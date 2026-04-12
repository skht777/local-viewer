//! インデクサーの内部ヘルパー関数
//!
//! スキャン・検索・UPSERT 等のインデクサー内部処理をまとめる。

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, params};

use crate::services::extensions::classify_for_index;
use crate::services::parallel_walk::WalkEntry;

use super::{IndexEntry, IndexerError, WalkCallbackArgs};

// --- 定数 ---

/// FTS5 trigram トークナイザが要求する最小文字数
const TRIGRAM_MIN_CHARS: usize = 3;

// --- 検索 ---

/// 検索結果の 1 件
pub(crate) struct SearchHit {
    pub relative_path: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
}

/// FTS5 MATCH による検索
#[allow(clippy::cast_possible_wrap, reason = "limit/offset は i64 範囲内")]
pub(super) fn search_fts(
    conn: &Connection,
    fts_query: &str,
    kind: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<SearchHit>, IndexerError> {
    let (sql, has_kind) = if kind.is_some() {
        (
            "SELECT e.relative_path, e.name, e.kind, e.size_bytes \
             FROM entries_fts f \
             JOIN entries e ON e.id = f.rowid \
             WHERE entries_fts MATCH ?1 AND e.kind = ?2 \
             LIMIT ?3 OFFSET ?4",
            true,
        )
    } else {
        (
            "SELECT e.relative_path, e.name, e.kind, e.size_bytes \
             FROM entries_fts f \
             JOIN entries e ON e.id = f.rowid \
             WHERE entries_fts MATCH ?1 \
             LIMIT ?2 OFFSET ?3",
            false,
        )
    };

    let mut stmt = conn.prepare(sql)?;

    let rows = if has_kind {
        let kind_val = kind.unwrap_or_default();
        stmt.query_map(
            params![fts_query, kind_val, limit as i64, offset as i64],
            map_search_hit,
        )?
    } else {
        stmt.query_map(
            params![fts_query, limit as i64, offset as i64],
            map_search_hit,
        )?
    };

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(IndexerError::from)
}

/// LIKE フォールバック検索 (2 文字クエリ等)
#[allow(clippy::cast_possible_wrap, reason = "limit/offset は i64 範囲内")]
pub(super) fn search_like(
    conn: &Connection,
    query: &str,
    kind: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<SearchHit>, IndexerError> {
    let pattern = format!("%{query}%");

    let (sql, has_kind) = if kind.is_some() {
        (
            "SELECT relative_path, name, kind, size_bytes \
             FROM entries \
             WHERE (name LIKE ?1 OR relative_path LIKE ?1) AND kind = ?2 \
             LIMIT ?3 OFFSET ?4",
            true,
        )
    } else {
        (
            "SELECT relative_path, name, kind, size_bytes \
             FROM entries \
             WHERE name LIKE ?1 OR relative_path LIKE ?1 \
             LIMIT ?2 OFFSET ?3",
            false,
        )
    };

    let mut stmt = conn.prepare(sql)?;

    let rows = if has_kind {
        let kind_val = kind.unwrap_or_default();
        stmt.query_map(
            params![pattern, kind_val, limit as i64, offset as i64],
            map_search_hit,
        )?
    } else {
        stmt.query_map(
            params![pattern, limit as i64, offset as i64],
            map_search_hit,
        )?
    };

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(IndexerError::from)
}

/// FTS5 クエリ文字列を組み立てる
///
/// スペース区切りで分割し、3 文字以上のトークンをダブルクォートで囲む。
/// 内部のダブルクォートは `""` にエスケープする。
/// トークン間はスペース (暗黙 AND) で結合する。
pub(super) fn build_fts_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .filter(|w| w.chars().count() >= TRIGRAM_MIN_CHARS)
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    tokens.join(" ")
}

/// `rusqlite::Row` から `SearchHit` にマッピングする
fn map_search_hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit {
        relative_path: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        size_bytes: row.get(3)?,
    })
}

// --- スキャン ---

/// `incremental_scan` の共有コンテキスト
///
/// `prune_unchanged_dir` と `process_walk_entry_incremental` が
/// 必要とするパラメータをまとめる。
pub(super) struct IncrementalScanContext<'a> {
    pub root_dir: &'a Path,
    pub mount_id: &'a str,
    pub conn: &'a Connection,
    pub existing: &'a BTreeMap<String, i64>,
    pub dir_mtimes: &'a HashMap<String, i64>,
    pub seen: &'a RefCell<HashSet<String>>,
    pub has_subdirs: &'a HashSet<String>,
}

/// mtime 未変更のディレクトリを枝刈りし、配下エントリを seen に追加する
///
/// `true` を返すと走査続行、`false` を返すと枝刈り。
pub(super) fn prune_unchanged_dir(
    path: &Path,
    mtime_ns: i64,
    ctx: &IncrementalScanContext<'_>,
) -> bool {
    let dir_relative = path
        .strip_prefix(ctx.root_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let relative_path = make_relative_prefix(ctx.mount_id, &dir_relative);
    let dir_key = relative_path.strip_suffix('/').unwrap_or(&relative_path);

    if let Some(&stored_mtime) = ctx.dir_mtimes.get(dir_key) {
        if stored_mtime == mtime_ns {
            // 子ディレクトリを持つ場合は枝刈りしない
            // (Unix mtime は直接の子のみ反映し、子孫の変更を検出するには再帰走査が必要)
            if ctx.has_subdirs.contains(dir_key) {
                return true;
            }
            // リーフディレクトリ: mtime 未変更 → 配下の既存エントリを全て seen に追加して枝刈り
            // BTreeMap::range で O(log n + k) のプレフィックスマッチ (k = マッチ数)
            let mut seen_mut = ctx.seen.borrow_mut();
            if dir_key.is_empty() {
                // ルートディレクトリ: 全エントリが対象
                for key in ctx.existing.keys() {
                    seen_mut.insert(key.clone());
                }
            } else {
                // dir_key 自体を seen に追加
                seen_mut.insert(dir_key.to_string());
                // "dir_key/" で始まるエントリを range で取得
                let prefix = format!("{dir_key}/");
                // prefix の次の境界値を計算 (最後のバイトをインクリメント)
                let mut end = prefix.clone().into_bytes();
                if let Some(last) = end.last_mut() {
                    *last += 1;
                }
                let end_str = String::from_utf8(end).unwrap_or_default();
                for (key, _) in ctx.existing.range(prefix..end_str) {
                    seen_mut.insert(key.clone());
                }
            }
            return false;
        }
    }
    true
}

/// `incremental_scan` 内の `WalkEntry` を処理し、(added, updated) を返す
pub(super) fn process_walk_entry_incremental(
    entry: &WalkEntry,
    ctx: &IncrementalScanContext<'_>,
    on_walk_entry: &mut Option<&mut dyn FnMut(WalkCallbackArgs)>,
) -> (usize, usize) {
    let dir_relative = entry
        .path
        .strip_prefix(ctx.root_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = make_relative_prefix(ctx.mount_id, &dir_relative);

    // コールバック通知
    if let Some(cb) = on_walk_entry {
        cb(WalkCallbackArgs {
            walk_entry_path: entry.path.to_string_lossy().into_owned(),
            root_dir: ctx.root_dir.to_string_lossy().into_owned(),
            mount_id: ctx.mount_id.to_string(),
            dir_mtime_ns: entry.mtime_ns,
            subdirs: entry.subdirs.clone(),
            files: entry.files.clone(),
        });
    }

    let mut added: usize = 0;
    let mut updated: usize = 0;

    // サブディレクトリを処理
    for (name, mtime_ns) in &entry.subdirs {
        if let Some(kind) = classify_for_index(name, true) {
            let relative_path = format!("{prefix}{name}");
            let ie = IndexEntry {
                relative_path: relative_path.clone(),
                name: name.clone(),
                kind: kind.to_string(),
                size_bytes: None,
                mtime_ns: *mtime_ns,
            };
            match upsert_entry(ctx.conn, &ie, ctx.existing) {
                Ok(UpsertResult::Added) => added += 1,
                Ok(UpsertResult::Updated) => updated += 1,
                Ok(UpsertResult::Unchanged) => {}
                Err(e) => tracing::error!("UPSERT 失敗: {e}"),
            }
            ctx.seen.borrow_mut().insert(relative_path);
        }
    }

    // ファイルを処理
    for (name, size_bytes, mtime_ns) in &entry.files {
        if let Some(kind) = classify_for_index(name, false) {
            let relative_path = format!("{prefix}{name}");
            let ie = IndexEntry {
                relative_path: relative_path.clone(),
                name: name.clone(),
                kind: kind.to_string(),
                size_bytes: Some(*size_bytes),
                mtime_ns: *mtime_ns,
            };
            match upsert_entry(ctx.conn, &ie, ctx.existing) {
                Ok(UpsertResult::Added) => added += 1,
                Ok(UpsertResult::Updated) => updated += 1,
                Ok(UpsertResult::Unchanged) => {}
                Err(e) => tracing::error!("UPSERT 失敗: {e}"),
            }
            ctx.seen.borrow_mut().insert(relative_path);
        }
    }

    (added, updated)
}

// --- バッチ・UPSERT ---

/// entries テーブルにバッチ INSERT する
pub(super) fn batch_insert(conn: &Connection, entries: &[IndexEntry]) -> Result<(), IndexerError> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO entries (relative_path, name, kind, size_bytes, mtime_ns) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for entry in entries {
            stmt.execute(params![
                entry.relative_path,
                entry.name,
                entry.kind,
                entry.size_bytes,
                entry.mtime_ns,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// `relative_path` プレフィックスを構築する
///
/// `mount_id/dir_relative/` の形式。ルート直下の場合は `mount_id/`。
pub(super) fn make_relative_prefix(mount_id: &str, dir_relative: &str) -> String {
    if dir_relative.is_empty() {
        format!("{mount_id}/")
    } else {
        format!("{mount_id}/{dir_relative}/")
    }
}

/// UPSERT の結果
enum UpsertResult {
    Added,
    Updated,
    Unchanged,
}

/// 既存エントリとの差分を判定して UPSERT する
fn upsert_entry(
    conn: &Connection,
    entry: &IndexEntry,
    existing: &BTreeMap<String, i64>,
) -> Result<UpsertResult, IndexerError> {
    if let Some(&stored_mtime) = existing.get(&entry.relative_path) {
        if stored_mtime == entry.mtime_ns {
            return Ok(UpsertResult::Unchanged);
        }
        // mtime が変わった → UPDATE
        conn.execute(
            "UPDATE entries SET name=?1, kind=?2, size_bytes=?3, mtime_ns=?4 \
             WHERE relative_path=?5",
            params![
                entry.name,
                entry.kind,
                entry.size_bytes,
                entry.mtime_ns,
                entry.relative_path,
            ],
        )?;
        Ok(UpsertResult::Updated)
    } else {
        // 新規 → INSERT
        conn.execute(
            "INSERT OR REPLACE INTO entries (relative_path, name, kind, size_bytes, mtime_ns) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.relative_path,
                entry.name,
                entry.kind,
                entry.size_bytes,
                entry.mtime_ns,
            ],
        )?;
        Ok(UpsertResult::Added)
    }
}

// --- データ読み込み ---

/// 既存エントリの (`relative_path`, `mtime_ns`) を `BTreeMap` に読み込む
///
/// `BTreeMap` を使用することで `prune_unchanged_dir` でのプレフィックスマッチを
/// `range()` で O(log n + k) に最適化できる（`HashMap` での O(n) 全走査を回避）。
pub(super) fn load_existing_entries(
    conn: &Connection,
) -> Result<BTreeMap<String, i64>, IndexerError> {
    let mut stmt = conn.prepare("SELECT relative_path, mtime_ns FROM entries")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (path, mtime) = row?;
        map.insert(path, mtime);
    }
    Ok(map)
}

/// ディレクトリエントリの (`relative_path`, `mtime_ns`) を `HashMap` に読み込む
pub(super) fn load_dir_mtimes(conn: &Connection) -> Result<HashMap<String, i64>, IndexerError> {
    let mut stmt =
        conn.prepare("SELECT relative_path, mtime_ns FROM entries WHERE kind = 'directory'")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, mtime) = row?;
        map.insert(path, mtime);
    }
    Ok(map)
}

/// `seen` に含まれないエントリを削除し、削除件数を返す
///
/// 一時テーブルに seen パスをバッチ INSERT し、NOT IN で一括 DELETE することで
/// 個別 DELETE の N 回の SQL 実行を 1 回に削減する。
#[allow(clippy::cast_sign_loss, reason = "削除件数は非負")]
pub(super) fn delete_unseen(
    conn: &Connection,
    seen: &HashSet<String>,
) -> Result<usize, IndexerError> {
    // 一時テーブルを作成し、seen パスを INSERT
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS seen_paths(path TEXT PRIMARY KEY);
         DELETE FROM seen_paths;",
    )?;

    // 一時テーブルは永続化不要のため、単一トランザクションで全パスを INSERT
    let tx = conn.unchecked_transaction()?;
    {
        let mut insert_stmt = conn.prepare("INSERT OR IGNORE INTO seen_paths(path) VALUES (?1)")?;
        for path in seen {
            insert_stmt.execute(params![path])?;
        }
    }
    tx.commit()?;

    // seen に含まれないエントリを一括削除
    let deleted = conn.execute(
        "DELETE FROM entries WHERE relative_path NOT IN (SELECT path FROM seen_paths)",
        [],
    )?;

    conn.execute("DROP TABLE IF EXISTS seen_paths", [])?;

    Ok(deleted)
}

// --- その他 ---

/// マウント ID をソートしてカンマ結合したフィンガープリントを生成する
pub(super) fn build_fingerprint(mount_ids: &[&str]) -> String {
    let mut sorted: Vec<&str> = mount_ids.to_vec();
    sorted.sort_unstable();
    sorted.join(",")
}
