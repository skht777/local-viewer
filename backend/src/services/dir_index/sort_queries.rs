//! ソートクエリ実装とヘルパー関数

use std::path::Path;

use rusqlite::{Connection, params};

use crate::services::extensions::{EntryKind, extract_extension};
use crate::services::indexer::WalkCallbackArgs;

use super::{DirEntry, DirIndexError};

pub(super) fn build_parent_path(args: &WalkCallbackArgs) -> String {
    let walk_path = Path::new(&args.walk_entry_path);
    let root_path = Path::new(&args.root_dir);

    let rel = walk_path
        .strip_prefix(root_path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    if rel.is_empty() || rel == "." {
        args.mount_id.clone()
    } else {
        format!("{}/{rel}", args.mount_id)
    }
}

/// ファイル名から種別文字列を返す (全種別を分類)
///
/// `DirIndex` は Indexer と異なり画像・other も含めて全エントリを格納する。
pub(super) fn classify_kind(name: &str) -> &'static str {
    let ext = extract_extension(name).to_lowercase();
    match EntryKind::from_extension(&ext) {
        EntryKind::Directory => "directory",
        EntryKind::Image => "image",
        EntryKind::Video => "video",
        EntryKind::Pdf => "pdf",
        EntryKind::Archive => "archive",
        EntryKind::Other => "other",
    }
}

/// `rusqlite::Row` から `DirEntry` にマッピングする
pub(super) fn map_dir_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<DirEntry> {
    Ok(DirEntry {
        parent_path: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        sort_key: row.get(3)?,
        size_bytes: row.get(4)?,
        mtime_ns: row.get(5)?,
    })
}

/// name-asc カーソルから `(kind_flag, sort_key)` を分離する
///
/// カーソル形式: `"{kind_flag}\x00{sort_key}"` (`kind_flag`: "0"=directory, "1"=other)
pub(super) fn parse_name_cursor(cursor: &str) -> (i64, &str) {
    if let Some(pos) = cursor.find('\x00') {
        let flag_str = &cursor[..pos];
        let sort_key = &cursor[pos + 1..];
        let flag: i64 = flag_str.parse().unwrap_or(1);
        (flag, sort_key)
    } else {
        // フォールバック: カーソル全体を sort_key として扱う
        (1, cursor)
    }
}

/// date カーソルから `(mtime_ns, sort_key)` を分離する
///
/// 新形式: `"{mtime_ns}\x00{sort_key}"` — タイブレーカー付き
/// 旧形式: `"{mtime_ns}"` — 後方互換 (`sort_key` なし)
pub(super) fn parse_date_cursor(cursor: &str) -> Result<(i64, Option<&str>), DirIndexError> {
    if let Some(pos) = cursor.find('\x00') {
        let mtime_str = &cursor[..pos];
        let sort_key = &cursor[pos + 1..];
        let mtime: i64 = mtime_str
            .parse()
            .map_err(|e| DirIndexError::Other(format!("無効な date カーソル: {e}")))?;
        Ok((mtime, Some(sort_key)))
    } else {
        // 旧形式: mtime_ns のみ
        let mtime: i64 = cursor
            .parse()
            .map_err(|e| DirIndexError::Other(format!("無効な date カーソル: {e}")))?;
        Ok((mtime, None))
    }
}

// ---------------------------------------------------------------
// ソートクエリ実装
// ---------------------------------------------------------------

/// ディレクトリ優先 + 名前昇順
///
/// `limit = None` は `SQLite` の `LIMIT -1` (無制限) にマップする。
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
pub(super) fn query_name_asc(
    conn: &Connection,
    parent_path: &str,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let sql_limit: i64 = limit.map_or(-1, |n| n as i64);
    let rows = if let Some(c) = cursor {
        let (kind_flag, sort_key) = parse_name_cursor(c);
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
               AND (CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key) > (?2, ?3) \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key ASC \
             LIMIT ?4",
        )?;
        stmt.query_map(
            params![parent_path, kind_flag, sort_key, sql_limit],
            map_dir_entry,
        )?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key ASC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, sql_limit], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

/// ディレクトリ優先 + 名前降順
///
/// `limit = None` は `SQLite` の `LIMIT -1` (無制限) にマップする。
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
pub(super) fn query_name_desc(
    conn: &Connection,
    parent_path: &str,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let sql_limit: i64 = limit.map_or(-1, |n| n as i64);
    let rows = if let Some(c) = cursor {
        let (kind_flag, sort_key) = parse_name_cursor(c);
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
               AND (CASE WHEN kind = 'directory' THEN 0 ELSE 1 END, sort_key) < (?2, ?3) \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END ASC, sort_key DESC \
             LIMIT ?4",
        )?;
        stmt.query_map(
            params![parent_path, kind_flag, sort_key, sql_limit],
            map_dir_entry,
        )?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY CASE WHEN kind = 'directory' THEN 0 ELSE 1 END ASC, sort_key DESC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, sql_limit], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

/// 日付降順 (新しい順)
///
/// `limit = None` は `SQLite` の `LIMIT -1` (無制限) にマップする。
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
pub(super) fn query_date_desc(
    conn: &Connection,
    parent_path: &str,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let sql_limit: i64 = limit.map_or(-1, |n| n as i64);
    let rows = if let Some(c) = cursor {
        let (mtime, sort_key) = parse_date_cursor(c)?;
        if let Some(sk) = sort_key {
            // タイブレーカー付きタプル比較: (mtime_ns DESC, sort_key ASC)
            let mut stmt = conn.prepare(
                "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
                 FROM dir_entries \
                 WHERE parent_path = ?1 \
                   AND (mtime_ns < ?2 OR (mtime_ns = ?2 AND sort_key > ?3)) \
                 ORDER BY mtime_ns DESC, sort_key ASC \
                 LIMIT ?4",
            )?;
            stmt.query_map(params![parent_path, mtime, sk, sql_limit], map_dir_entry)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            // 旧形式: mtime_ns のみ比較 (後方互換)
            let mut stmt = conn.prepare(
                "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
                 FROM dir_entries \
                 WHERE parent_path = ?1 AND mtime_ns < ?2 \
                 ORDER BY mtime_ns DESC, sort_key ASC \
                 LIMIT ?3",
            )?;
            stmt.query_map(params![parent_path, mtime, sql_limit], map_dir_entry)?
                .collect::<Result<Vec<_>, _>>()?
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY mtime_ns DESC, sort_key ASC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, sql_limit], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

/// 日付昇順 (古い順)
///
/// `limit = None` は `SQLite` の `LIMIT -1` (無制限) にマップする。
#[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
pub(super) fn query_date_asc(
    conn: &Connection,
    parent_path: &str,
    limit: Option<usize>,
    cursor: Option<&str>,
) -> Result<Vec<DirEntry>, DirIndexError> {
    let sql_limit: i64 = limit.map_or(-1, |n| n as i64);
    let rows = if let Some(c) = cursor {
        let (mtime, sort_key) = parse_date_cursor(c)?;
        if let Some(sk) = sort_key {
            // タイブレーカー付きタプル比較: (mtime_ns ASC, sort_key ASC)
            let mut stmt = conn.prepare(
                "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
                 FROM dir_entries \
                 WHERE parent_path = ?1 \
                   AND (mtime_ns > ?2 OR (mtime_ns = ?2 AND sort_key > ?3)) \
                 ORDER BY mtime_ns ASC, sort_key ASC \
                 LIMIT ?4",
            )?;
            stmt.query_map(params![parent_path, mtime, sk, sql_limit], map_dir_entry)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            // 旧形式: mtime_ns のみ比較 (後方互換)
            let mut stmt = conn.prepare(
                "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
                 FROM dir_entries \
                 WHERE parent_path = ?1 AND mtime_ns > ?2 \
                 ORDER BY mtime_ns ASC, sort_key ASC \
                 LIMIT ?3",
            )?;
            stmt.query_map(params![parent_path, mtime, sql_limit], map_dir_entry)?
                .collect::<Result<Vec<_>, _>>()?
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 \
             ORDER BY mtime_ns ASC, sort_key ASC \
             LIMIT ?2",
        )?;
        stmt.query_map(params![parent_path, sql_limit], map_dir_entry)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

// ===================================================================
// sibling クエリ実装
// ===================================================================

/// name 系ソートでの sibling クエリ
///
/// browse クエリと同じ複合ソート `(kind != 'directory', sort_key)` で比較する。
/// 混合方向のため、明示的 OR 条件でタプル比較を表現。
#[allow(clippy::too_many_arguments, reason = "sort 分岐に必要なパラメータ群")]
pub(super) fn query_sibling_name(
    conn: &Connection,
    parent_path: &str,
    current_sort_key: &str,
    current_is_dir: bool,
    direction: &str,
    sort: &str,
    kinds: &[&str],
) -> Result<Option<DirEntry>, DirIndexError> {
    let placeholders: Vec<String> = (0..kinds.len()).map(|i| format!("?{}", i + 4)).collect();
    let in_clause = placeholders.join(", ");

    let current_kind_flag: i64 = i64::from(!current_is_dir);
    let is_asc = sort == "name-asc";
    let is_next = direction == "next";

    // browse クエリの複合ソート: (kind != 'directory') ASC, sort_key ASC/DESC
    let (cmp, order) = match (is_asc, is_next) {
        (true, true) => (
            "((kind != 'directory') > ?2 OR ((kind != 'directory') = ?2 AND sort_key > ?3))",
            "(kind != 'directory') ASC, sort_key ASC",
        ),
        (true, false) => (
            "((kind != 'directory') < ?2 OR ((kind != 'directory') = ?2 AND sort_key < ?3))",
            "(kind != 'directory') DESC, sort_key DESC",
        ),
        (false, true) => (
            "((kind != 'directory') > ?2 OR ((kind != 'directory') = ?2 AND sort_key < ?3))",
            "(kind != 'directory') ASC, sort_key DESC",
        ),
        (false, false) => (
            "((kind != 'directory') < ?2 OR ((kind != 'directory') = ?2 AND sort_key > ?3))",
            "(kind != 'directory') DESC, sort_key ASC",
        ),
    };

    let sql = format!(
        "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
         FROM dir_entries \
         WHERE parent_path = ?1 AND kind IN ({in_clause}) AND {cmp} \
         ORDER BY {order} \
         LIMIT 1"
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params_vec.push(Box::new(parent_path.to_string()));
    params_vec.push(Box::new(current_kind_flag));
    params_vec.push(Box::new(current_sort_key.to_string()));
    for kind in kinds {
        params_vec.push(Box::new(kind.to_string()));
    }
    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        params_vec.iter().map(std::convert::AsRef::as_ref).collect();

    let mut rows = stmt.query_map(params_ref.as_slice(), map_dir_entry)?;
    match rows.next() {
        Some(Ok(entry)) => Ok(Some(entry)),
        Some(Err(e)) => Err(DirIndexError::from(e)),
        None => Ok(None),
    }
}

/// date 系ソートでの sibling クエリ
///
/// `name` カラムで逆引き (`UNIQUE(parent_path, name)` が保証)。
/// Windows Explorer 準拠の正準順序: `(mtime_ns, sort_key ASC)`
pub(super) fn query_sibling_date(
    conn: &Connection,
    parent_path: &str,
    current_name: &str,
    direction: &str,
    sort: &str,
    kinds: &[&str],
) -> Result<Option<DirEntry>, DirIndexError> {
    // name で逆引き (sort_key は大文字小文字衝突のため使わない)
    let cur_row: Option<(i64, String)> = conn
        .query_row(
            "SELECT mtime_ns, sort_key FROM dir_entries \
             WHERE parent_path = ?1 AND name = ?2 LIMIT 1",
            params![parent_path, current_name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let Some((current_mtime, current_sort_key)) = cur_row else {
        return Ok(None);
    };

    let placeholders: Vec<String> = (0..kinds.len()).map(|i| format!("?{}", i + 4)).collect();
    let in_clause = placeholders.join(", ");

    let is_asc = sort == "date-asc";
    let is_next = direction == "next";

    // (mtime_ns, sort_key ASC) タプル比較
    let (cmp, order) = match (is_asc, is_next) {
        (true, true) => (
            "(mtime_ns > ?2 OR (mtime_ns = ?2 AND sort_key > ?3))",
            "mtime_ns ASC, sort_key ASC",
        ),
        (true, false) => (
            "(mtime_ns < ?2 OR (mtime_ns = ?2 AND sort_key < ?3))",
            "mtime_ns DESC, sort_key DESC",
        ),
        (false, true) => (
            "(mtime_ns < ?2 OR (mtime_ns = ?2 AND sort_key > ?3))",
            "mtime_ns DESC, sort_key ASC",
        ),
        (false, false) => (
            "(mtime_ns > ?2 OR (mtime_ns = ?2 AND sort_key < ?3))",
            "mtime_ns ASC, sort_key DESC",
        ),
    };

    let sql = format!(
        "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
         FROM dir_entries \
         WHERE parent_path = ?1 AND kind IN ({in_clause}) AND {cmp} \
         ORDER BY {order} \
         LIMIT 1"
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params_vec.push(Box::new(parent_path.to_string()));
    params_vec.push(Box::new(current_mtime));
    params_vec.push(Box::new(current_sort_key));
    for kind in kinds {
        params_vec.push(Box::new(kind.to_string()));
    }
    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        params_vec.iter().map(std::convert::AsRef::as_ref).collect();

    let mut rows = stmt.query_map(params_ref.as_slice(), map_dir_entry)?;
    match rows.next() {
        Some(Ok(entry)) => Ok(Some(entry)),
        Some(Err(e)) => Err(DirIndexError::from(e)),
        None => Ok(None),
    }
}

// ===================================================================
// テスト
// ===================================================================
