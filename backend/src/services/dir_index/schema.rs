//! `DirIndex` のテーブル/インデックス初期化とスキーママイグレーション

use rusqlite::params;

use super::{DirIndex, DirIndexError, SCHEMA_VERSION};

impl DirIndex {
    /// テーブルとインデックスを作成する (冪等)
    pub(crate) fn init_db(&self) -> Result<(), DirIndexError> {
        let conn = self.connect()?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_meta (\
                 key TEXT PRIMARY KEY, \
                 value TEXT NOT NULL\
             );\
             \
             CREATE TABLE IF NOT EXISTS dir_entries (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 parent_path TEXT NOT NULL, \
                 name TEXT NOT NULL, \
                 kind TEXT NOT NULL, \
                 sort_key TEXT NOT NULL, \
                 size_bytes INTEGER, \
                 mtime_ns INTEGER NOT NULL, \
                 UNIQUE(parent_path, name)\
             );\
             \
             CREATE INDEX IF NOT EXISTS idx_dir_parent \
                 ON dir_entries(parent_path);\
             CREATE INDEX IF NOT EXISTS idx_dir_parent_sort \
                 ON dir_entries(parent_path, sort_key);\
             CREATE INDEX IF NOT EXISTS idx_dir_parent_mtime \
                 ON dir_entries(parent_path, mtime_ns);\
             \
             CREATE TABLE IF NOT EXISTS dir_meta (\
                 path TEXT PRIMARY KEY, \
                 mtime_ns INTEGER NOT NULL\
             );",
        )?;

        // sort_key 形式変更時のマイグレーション (v1→v2: ゼロ埋め 10桁→20桁)
        // 旧形式の sort_key が残っていると DirIndex パスと非 DirIndex パスで
        // ソート順が不一致になるため、dir_entries を全削除して再スキャンを促す
        let tx = conn.unchecked_transaction()?;
        let old_version: Option<String> = tx
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .ok();

        if old_version.as_deref() != Some(SCHEMA_VERSION) {
            tx.execute("DELETE FROM dir_entries", [])?;
            tx.execute("DELETE FROM dir_meta", [])?;
            tx.execute("DELETE FROM schema_meta WHERE key = 'full_scan_done'", [])?;
            tracing::info!(
                old = old_version.as_deref().unwrap_or("none"),
                new = SCHEMA_VERSION,
                "DirIndex スキーマバージョン変更: dir_entries + dir_meta をクリア (フルスキャン強制)"
            );
        }

        tx.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES (?1, ?2)",
            params![&"schema_version", SCHEMA_VERSION],
        )?;
        tx.commit()?;

        Ok(())
    }
}
