//! インデクサーの内部ヘルパー関数
//!
//! スキャン・検索・UPSERT 等のインデクサー内部処理をまとめる。

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use rusqlite::{Connection, params};

use crate::services::extensions::classify_for_index;
use crate::services::parallel_walk::WalkEntry;

use super::{IndexEntry, IndexerError, WalkCallbackArgs};

// --- 定数 ---

/// FTS5 trigram トークナイザが要求する最小文字数
const TRIGRAM_MIN_CHARS: usize = 3;

// --- LIKE エスケープ ---

/// LIKE パターンのワイルドカード文字をエスケープする
///
/// `\`, `%`, `_` をバックスラッシュでエスケープし、
/// `LIKE ? ESCAPE '\'` と組み合わせて安全にプレフィックスマッチを行う。
pub(super) fn escape_like_pattern(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '%' | '_' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

// --- 検索 ---

/// 検索結果の 1 件
pub(crate) struct SearchHit {
    pub relative_path: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
}

/// キーワード検索を実行する
///
/// クエリをスペース区切りで分割し、トークンごとに以下のようにルーティングする:
/// - 3 文字以上: FTS5 MATCH (trigram インデックス)
/// - 2 文字以下: `name LIKE %t% OR relative_path LIKE %t%`
///
/// 全トークンを SQL 上で AND 結合して、日本語 2 文字名詞 + 複数キーワードの
/// 実用的な検索に対応する。
///
/// - `scope_range` が指定された場合、`relative_path >= ?lo AND < ?hi` で
///   インデックス利用可能な range scan によりスコープ限定する（BINARY collation 前提）
/// - FTS5 トークンが 1 つ以上ある場合は `entries_fts JOIN entries`、
///   全てが LIKE の場合は `entries` 直接の動的 SQL を構築する
pub(super) fn search_combined(
    conn: &Connection,
    query: &str,
    kind: Option<&str>,
    limit: usize,
    offset: usize,
    scope_range: Option<(&str, &str)>,
) -> Result<Vec<SearchHit>, IndexerError> {
    let (fts_tokens, like_tokens): (Vec<&str>, Vec<&str>) = query
        .split_whitespace()
        .partition(|t| t.chars().count() >= TRIGRAM_MIN_CHARS);

    if fts_tokens.is_empty() && like_tokens.is_empty() {
        return Ok(Vec::new());
    }

    let use_fts = !fts_tokens.is_empty();

    // LIKE トークンはワイルドカードをエスケープして `%t%` パターンを構築
    let like_patterns: Vec<String> = like_tokens
        .iter()
        .map(|t| format!("%{}%", escape_like_pattern(t)))
        .collect();

    // SQL と bind パラメータを動的に構築（bind のみ使用、文字列埋め込み禁止）
    let mut sql = if use_fts {
        String::from(
            "SELECT e.relative_path, e.name, e.kind, e.size_bytes \
             FROM entries_fts f \
             JOIN entries e ON e.id = f.rowid \
             WHERE entries_fts MATCH ?1",
        )
    } else {
        String::from("SELECT relative_path, name, kind, size_bytes FROM entries WHERE 1=1")
    };
    let col_prefix = if use_fts { "e." } else { "" };
    let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx: usize = 1;

    if use_fts {
        bind_values.push(Box::new(build_fts_match_query(&fts_tokens)));
        param_idx += 1;
    }

    for pattern in &like_patterns {
        let _ = write!(
            sql,
            " AND ({col_prefix}name LIKE ?{param_idx} ESCAPE '\\' OR \
              {col_prefix}relative_path LIKE ?{param_idx} ESCAPE '\\')"
        );
        bind_values.push(Box::new(pattern.clone()));
        param_idx += 1;
    }

    if let Some(k) = kind {
        let _ = write!(sql, " AND {col_prefix}kind = ?{param_idx}");
        bind_values.push(Box::new(k.to_string()));
        param_idx += 1;
    }

    if let Some((lo, hi)) = scope_range {
        // BETWEEN 形式で range scan (SCAN ではなく SEARCH USING INDEX になる)
        let lo_idx = param_idx;
        let hi_idx = param_idx + 1;
        let _ = write!(
            sql,
            " AND {col_prefix}relative_path >= ?{lo_idx} \
               AND {col_prefix}relative_path < ?{hi_idx}"
        );
        bind_values.push(Box::new(lo.to_string()));
        bind_values.push(Box::new(hi.to_string()));
        param_idx += 2;
    }

    let _ = write!(sql, " LIMIT ?{param_idx} OFFSET ?{}", param_idx + 1);
    // usize → i64: LIMIT/OFFSET は実用範囲内なので try_from でクランプ
    bind_values.push(Box::new(i64::try_from(limit).unwrap_or(i64::MAX)));
    bind_values.push(Box::new(i64::try_from(offset).unwrap_or(i64::MAX)));

    let mut stmt = conn.prepare(&sql)?;
    let bind_refs: Vec<&dyn rusqlite::types::ToSql> =
        bind_values.iter().map(AsRef::as_ref).collect();
    let rows = stmt.query_map(bind_refs.as_slice(), map_search_hit)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(IndexerError::from)
}

/// FTS5 MATCH 用のクエリ文字列を組み立てる
///
/// 各トークンをダブルクォートで囲み、内部のダブルクォートは `""` にエスケープする。
/// トークン間はスペース (暗黙 AND) で結合する。
fn build_fts_match_query(tokens: &[&str]) -> String {
    tokens
        .iter()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
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
/// 必要とするパラメータをまとめる。`upsert_errors` は UPSERT 失敗件数を
/// 集計し、呼び出し側が `delete_unseen` を実行するか判断する際に参照する。
pub(super) struct IncrementalScanContext<'a> {
    pub root_dir: &'a Path,
    pub mount_id: &'a str,
    pub conn: &'a Connection,
    pub existing: &'a BTreeMap<String, i64>,
    pub dir_mtimes: &'a HashMap<String, i64>,
    pub seen: &'a RefCell<HashSet<String>>,
    pub has_subdirs: &'a HashSet<String>,
    pub upsert_errors: &'a RefCell<usize>,
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
                Err(e) => {
                    tracing::error!("UPSERT 失敗: {e}");
                    *ctx.upsert_errors.borrow_mut() += 1;
                }
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
                Err(e) => {
                    tracing::error!("UPSERT 失敗: {e}");
                    *ctx.upsert_errors.borrow_mut() += 1;
                }
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

/// `mount_id` 配下の entries を range scan するための `(lo, hi)` を返す
///
/// - **invariant**: `mount_id` は lowercase hex 16 桁 (`[0-9a-f]{16}`) を想定
///   （HMAC-SHA256 先頭 16 桁で生成される前提、`node_registry::build_mount_id` と整合）
/// - `entries.relative_path` の collation 既定 (BINARY) での range scan を前提に
///   lexicographic な範囲 `[lo, hi)` を返す
/// - `/` (ASCII 0x2F) の次の文字 `0` (0x30) で終端を閉じる
///   （`mount_id` に `/` `0` より小さい文字が含まれないことが invariant で保証される）
/// - invariant 違反 (空 / 長さ不一致 / 非 hex) は `IndexerError::Other` で reject
///   することで、`mount1/photos` 等のネスト prefix 経路との混同を防ぐ
pub(super) fn mount_scope_range(mount_id: &str) -> Result<(String, String), IndexerError> {
    fn is_valid(id: &str) -> bool {
        id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }
    if !is_valid(mount_id) {
        return Err(IndexerError::Other(format!(
            "mount_id invariant 違反 (lowercase hex 16 桁): len={}",
            mount_id.len()
        )));
    }
    Ok((format!("{mount_id}/"), format!("{mount_id}0")))
}

/// search の `scope_prefix` 用の range `(lo, hi)` を返す
///
/// - 入力: ワイルドカード非含みの literal prefix。末尾 `/` を含めない生の prefix
///   を想定（例: `mount1/photos`, `mount/dir_100%`）
/// - `(lo, hi)` = `(format!("{prefix}/"), format!("{prefix}0"))`
///   - ASCII 順で `/` (0x2F) の次は `0` (0x30)、`{prefix}/` 以降のすべての
///     キーはこの半開区間に収まる（`{prefix}0` は `{prefix}/...` よりも大きい）
/// - `mount_scope_range` と違い、この関数は invariant を課さない
///   （`mount_id` 専用ではないため）
/// - `%` `_` `\` を含む literal prefix でも escape 不要で range に乗る
pub(super) fn prefix_scope_range(prefix: &str) -> (String, String) {
    (format!("{prefix}/"), format!("{prefix}0"))
}

/// 既存エントリの (`relative_path`, `mtime_ns`) を `BTreeMap` に読み込む
///
/// - 指定された `mount_id` プレフィックス配下のみを対象とする
///   （他マウントの行を混ぜると `delete_unseen` が誤削除するため）
/// - `BTreeMap` を使用することで `prune_unchanged_dir` でのプレフィックスマッチを
///   `range()` で O(log n + k) に最適化できる（`HashMap` での O(n) 全走査を回避）。
/// - SQL 側は `mount_scope_range` による BETWEEN 形式で range scan（`SCAN entries`
///   ではなく `SEARCH ... USING INDEX idx_entries_relative_path` になる）
pub(super) fn load_existing_entries(
    conn: &Connection,
    mount_id: &str,
) -> Result<BTreeMap<String, i64>, IndexerError> {
    let (scope_lo, scope_hi) = mount_scope_range(mount_id)?;
    let mut stmt = conn.prepare(
        "SELECT relative_path, mtime_ns FROM entries \
         WHERE relative_path >= ?1 AND relative_path < ?2",
    )?;
    let rows = stmt.query_map(params![scope_lo, scope_hi], |row| {
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
///
/// 指定された `mount_id` プレフィックス配下のみを対象とする。`mount_scope_range`
/// による BETWEEN 形式で range scan する。
pub(super) fn load_dir_mtimes(
    conn: &Connection,
    mount_id: &str,
) -> Result<HashMap<String, i64>, IndexerError> {
    let (scope_lo, scope_hi) = mount_scope_range(mount_id)?;
    let mut stmt = conn.prepare(
        "SELECT relative_path, mtime_ns FROM entries \
         WHERE kind = 'directory' \
           AND relative_path >= ?1 AND relative_path < ?2",
    )?;
    let rows = stmt.query_map(params![scope_lo, scope_hi], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, mtime) = row?;
        map.insert(path, mtime);
    }
    Ok(map)
}

/// `seen` に含まれない指定マウントのエントリを削除し、削除件数を返す
///
/// - 削除対象は **指定 `mount_id` プレフィックス配下**に限定する
///   （他マウントのエントリを巻き込んで削除する cross-mount データロスを防止）
/// - `seen_paths` は**接続ローカルの TEMP TABLE**。同一接続で per-mount の
///   `delete_unseen` を逐次呼ぶ場合、各呼び出しの冒頭 `DELETE FROM seen_paths`
///   でリセットされる前提（前マウント分の seen を混ぜない）
/// - 一時テーブルに seen パスをバッチ INSERT し、NOT IN で一括 DELETE することで
///   個別 DELETE の N 回の SQL 実行を 1 回に削減する。
/// - `mount_scope_range` による BETWEEN で range scan（SCAN entries にならない）
pub(super) fn delete_unseen(
    conn: &Connection,
    seen: &HashSet<String>,
    mount_id: &str,
) -> Result<usize, IndexerError> {
    let (scope_lo, scope_hi) = mount_scope_range(mount_id)?;

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

    // seen に含まれず、かつ自マウント配下のエントリを一括削除
    let deleted = conn.execute(
        "DELETE FROM entries \
         WHERE relative_path >= ?1 AND relative_path < ?2 \
           AND relative_path NOT IN (SELECT path FROM seen_paths)",
        params![scope_lo, scope_hi],
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

// --- 削除スキップ判定 ---

/// 絶対値の閾値: このエラー件数以上で削除をスキップ（= `observed_entries` が
/// 100 未満の小規模でも、1 件エラーがあればスキップする根拠に使う）
pub(super) const MAX_ABS_SKIP_THRESHOLD: usize = 100;

/// 率の閾値: `observed_entries` に対してこの割合以上でスキップ（1%）
pub(super) const MAX_RATE_SKIP_THRESHOLD: f64 = 0.01;

/// `incremental_scan` / `rebuild` の削除フェーズをスキップすべきか判定する
///
/// ハイブリッド閾値（絶対 + 率）による保守的な判定:
/// - `total_errors == 0` → 削除実行（false）
/// - `observed_entries < MAX_ABS_SKIP_THRESHOLD` (小規模) → 1 件でもスキップ（true）
/// - 大規模では `total_errors >= MAX_ABS_SKIP_THRESHOLD` または
///   エラー率 `>= MAX_RATE_SKIP_THRESHOLD` でスキップ
///
/// 引数:
/// - `walk_errors`: `WalkReport.error_count()` の真値（サンプル cap とは独立）
/// - `upsert_errors`: `IncrementalScanContext.upsert_errors` の集計値
/// - `observed_entries`: `WalkReport.observed_entries`
///   (`visited_dirs + visible_children + 各種失敗件数` の合算、実試行数)
pub(super) fn should_skip_delete(
    walk_errors: usize,
    upsert_errors: usize,
    observed_entries: usize,
) -> bool {
    let total_errors = walk_errors + upsert_errors;
    if total_errors == 0 {
        return false;
    }
    // 小規模は 1 件でもスキップ（既存テスト互換 + 試行数が少ない時に率判定は
    // ノイズに弱い）
    if observed_entries < MAX_ABS_SKIP_THRESHOLD {
        return true;
    }
    // 大規模: 絶対閾値 or 率のどちらかを超えたらスキップ
    if total_errors >= MAX_ABS_SKIP_THRESHOLD {
        return true;
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "usize → f64 の精度損失は実運用の規模 (< 10^15) で影響しない"
    )]
    let rate = total_errors as f64 / observed_entries as f64;
    rate >= MAX_RATE_SKIP_THRESHOLD
}

#[cfg(test)]
mod skip_delete_tests {
    use super::*;

    #[test]
    fn should_skip_deleteはエラー0で削除許可する() {
        assert!(!should_skip_delete(0, 0, 100_000));
    }

    #[test]
    fn should_skip_deleteは小規模エラー1件でもスキップする() {
        // 既存テスト挙動互換: observed < 100 の小規模は 1 件でもスキップ
        assert!(should_skip_delete(1, 0, 10));
        // upsert エラーのみでも同様
        assert!(should_skip_delete(0, 1, 10));
    }

    #[test]
    fn should_skip_deleteは境界99_100_101で正しく分岐する() {
        // observed=99, total_errors=1 → 小規模分岐で true (スキップ)
        assert!(should_skip_delete(1, 0, 99));
        // observed=100, total_errors=1 → 小規模分岐を抜ける、率 1% 未満 かつ
        //   total_errors < MAX_ABS_SKIP_THRESHOLD → false (削除)
        // ただし 1/100 = 0.01 は >= 1% なので true (スキップ)
        assert!(should_skip_delete(1, 0, 100));
        // observed=101, total_errors=2 → rate=2/101 ≈ 1.98% > 1% → true (スキップ)
        assert!(should_skip_delete(2, 0, 101));
    }

    #[test]
    fn should_skip_deleteは大規模で率1percent未満なら削除を許可する() {
        // observed=100_000, total_errors=50 → rate=0.05%, total<100 (絶対閾値未満)
        // → false (削除を許可)。単発の transient エラーで stale 行が溜まるのを防ぐ。
        assert!(!should_skip_delete(50, 0, 100_000));
    }

    #[test]
    fn should_skip_deleteは絶対値100件超でスキップする() {
        // observed=1_000_000, total_errors=150 → rate=0.015% < 1% だが
        // total >= MAX_ABS_SKIP_THRESHOLD (100) → true (スキップ)
        assert!(should_skip_delete(150, 0, 1_000_000));
    }

    #[test]
    fn should_skip_deleteはwalk_errorsとupsert_errorsを合算する() {
        // 合算で 100 件以上 → 絶対閾値ヒットでスキップ
        assert!(should_skip_delete(60, 40, 1_000_000));
        // 合算で 99 件 → 絶対閾値未満、率 0.0099% < 1% → 削除許可
        assert!(!should_skip_delete(50, 49, 1_000_000));
    }
}
