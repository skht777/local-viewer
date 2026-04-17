//! `DirIndexReader` の読み取りクエリ
//!
//! - `query_page`: ソート + カーソルベースページネーション
//! - `child_count` / `preview_entries` / `first_entry_by_kind` / `query_sibling` / `entry_count`
//! - `get_dir_mtime` / `batch_dir_info`
//!
//! SQL 本体は `sort_queries.rs` の `query_name_*` / `query_date_*` / `query_sibling_*` に委譲。

use rusqlite::params;

use crate::services::natural_sort::encode_sort_key;

use super::sort_queries::{
    map_dir_entry, query_date_asc, query_date_desc, query_name_asc, query_name_desc,
    query_sibling_date, query_sibling_name,
};
use super::{DirChildInfo, DirEntry, DirIndexError, DirIndexReader};

impl DirIndexReader<'_> {
    /// ソート + カーソルベースページネーション付きでエントリを返す
    ///
    /// `limit = None` は全件取得 (`SQLite` `LIMIT -1` 相当)
    pub(crate) fn query_page(
        &self,
        parent_path: &str,
        sort: &str,
        limit: Option<usize>,
        cursor_sort_key: Option<&str>,
    ) -> Result<Vec<DirEntry>, DirIndexError> {
        match sort {
            "name-desc" => query_name_desc(&self.conn, parent_path, limit, cursor_sort_key),
            "date-desc" => query_date_desc(&self.conn, parent_path, limit, cursor_sort_key),
            "date-asc" => query_date_asc(&self.conn, parent_path, limit, cursor_sort_key),
            _ => query_name_asc(&self.conn, parent_path, limit, cursor_sort_key),
        }
    }

    /// 指定ディレクトリの子エントリ数を返す
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn child_count(&self, parent_path: &str) -> Result<usize, DirIndexError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM dir_entries WHERE parent_path = ?1",
            params![parent_path],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// サムネイル対象エントリを返す (画像/動画/PDF/アーカイブ)
    #[allow(clippy::cast_possible_wrap, reason = "limit は i64 範囲内")]
    pub(crate) fn preview_entries(
        &self,
        parent_path: &str,
        limit: usize,
    ) -> Result<Vec<DirEntry>, DirIndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 AND kind IN ('image', 'archive', 'pdf', 'video') \
             ORDER BY sort_key ASC \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![parent_path, limit as i64], map_dir_entry)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DirIndexError::from)
    }

    /// 指定 kind の最初のエントリを返す (`first-viewable` 高速パス用)
    pub(crate) fn first_entry_by_kind(
        &self,
        parent_path: &str,
        kind: &str,
    ) -> Result<Option<DirEntry>, DirIndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns \
             FROM dir_entries \
             WHERE parent_path = ?1 AND kind = ?2 \
             ORDER BY sort_key ASC \
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![parent_path, kind], map_dir_entry)?;
        match rows.next() {
            Some(Ok(entry)) => Ok(Some(entry)),
            Some(Err(e)) => Err(DirIndexError::from(e)),
            None => Ok(None),
        }
    }

    /// 次/前の兄弟エントリを返す (`sibling` 高速パス用)
    #[allow(clippy::too_many_arguments, reason = "sort 分岐に必要なパラメータ群")]
    pub(crate) fn query_sibling(
        &self,
        parent_path: &str,
        current_name: &str,
        current_is_dir: bool,
        direction: &str,
        sort: &str,
        kinds: &[&str],
    ) -> Result<Option<DirEntry>, DirIndexError> {
        let current_sort_key = encode_sort_key(current_name);

        match sort {
            "name-asc" | "name-desc" => query_sibling_name(
                &self.conn,
                parent_path,
                &current_sort_key,
                current_is_dir,
                direction,
                sort,
                kinds,
            ),
            "date-asc" | "date-desc" => query_sibling_date(
                &self.conn,
                parent_path,
                current_name,
                direction,
                sort,
                kinds,
            ),
            _ => query_sibling_name(
                &self.conn,
                parent_path,
                &current_sort_key,
                current_is_dir,
                direction,
                "name-asc",
                kinds,
            ),
        }
    }

    /// DB 内の全エントリ数を返す
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn entry_count(&self) -> Result<usize, DirIndexError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM dir_entries", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// ディレクトリの記録済み mtime を返す
    #[allow(
        clippy::unnecessary_wraps,
        reason = "DirIndex::get_dir_mtime と同じシグネチャを維持"
    )]
    pub(crate) fn get_dir_mtime(&self, path: &str) -> Result<Option<i64>, DirIndexError> {
        let result = self
            .conn
            .query_row(
                "SELECT mtime_ns FROM dir_meta WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// 複数ディレクトリの `child_count` + `preview_entries` を一括取得する
    ///
    /// `parent_keys` の各ディレクトリについて `DirChildInfo` を返す。
    /// 存在しないキーは結果に含まれない。
    #[allow(clippy::cast_sign_loss, reason = "COUNT(*) は非負")]
    pub(crate) fn batch_dir_info(
        &self,
        parent_keys: &[&str],
        preview_limit: usize,
    ) -> Result<std::collections::HashMap<String, DirChildInfo>, DirIndexError> {
        use std::collections::HashMap;

        if parent_keys.is_empty() {
            return Ok(HashMap::new());
        }

        // 動的 IN 句のプレースホルダ構築
        let placeholders: Vec<String> = (1..=parent_keys.len()).map(|i| format!("?{i}")).collect();
        let in_clause = placeholders.join(", ");

        // child_count をバッチ取得
        let count_sql = format!(
            "SELECT parent_path, COUNT(*) FROM dir_entries \
             WHERE parent_path IN ({in_clause}) GROUP BY parent_path"
        );
        let mut count_stmt = self.conn.prepare(&count_sql)?;
        let params_vec: Vec<&dyn rusqlite::types::ToSql> = parent_keys
            .iter()
            .map(|k| k as &dyn rusqlite::types::ToSql)
            .collect();
        let count_rows = count_stmt.query_map(params_vec.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut result: HashMap<String, DirChildInfo> = HashMap::new();
        for row in count_rows {
            let (path, count) = row?;
            result.insert(
                path,
                DirChildInfo {
                    count: count as usize,
                    previews: Vec::new(),
                },
            );
        }

        // preview_entries をウィンドウ関数でバッチ取得
        #[allow(
            clippy::cast_possible_wrap,
            reason = "preview_limit は小さい値 (通常3)"
        )]
        let limit_i64 = preview_limit as i64;
        let preview_sql = format!(
            "SELECT parent_path, name, kind, sort_key, size_bytes, mtime_ns FROM (\
                 SELECT *, ROW_NUMBER() OVER (\
                     PARTITION BY parent_path ORDER BY sort_key ASC\
                 ) AS rn \
                 FROM dir_entries \
                 WHERE parent_path IN ({in_clause}) \
                   AND kind IN ('image', 'archive', 'pdf', 'video')\
             ) WHERE rn <= ?{}",
            parent_keys.len() + 1
        );
        let mut preview_stmt = self.conn.prepare(&preview_sql)?;
        let mut preview_params: Vec<&dyn rusqlite::types::ToSql> = parent_keys
            .iter()
            .map(|k| k as &dyn rusqlite::types::ToSql)
            .collect();
        preview_params.push(&limit_i64);
        let preview_rows = preview_stmt.query_map(preview_params.as_slice(), map_dir_entry)?;

        for row in preview_rows {
            let entry = row?;
            let key = entry.parent_path.clone();
            result
                .entry(key)
                .or_insert_with(|| DirChildInfo {
                    count: 0,
                    previews: Vec::new(),
                })
                .previews
                .push(entry);
        }

        Ok(result)
    }
}
