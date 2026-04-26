//! `DirIndex` の書き込み操作
//!
//! - `ingest_walk_entry`: `parallel_walk` の 1 エントリを DB に格納
//! - `set_dir_mtime`: ディレクトリの mtime を `dir_meta` に記録
//! - フルスキャン完了フラグ (`is_full_scan_done` / `mark_full_scan_done`)
//! - `begin_bulk`: `BulkInserter` を開く (`synchronous=OFF` で高速格納)
//! - `canonicalize_parent_in_tx`: per-parent cascade canonical replace
//! - `recover_from_corrupt_persistent_name`: 永続層破損リカバリヘルパ

use rusqlite::{Connection, Transaction, params};

use crate::services::indexer::{Indexer, WalkCallbackArgs};
use crate::services::natural_sort::encode_sort_key;
use crate::services::path_keys::mount_scope_range;

use super::sort_queries::{build_parent_path, classify_kind};
use super::{BulkInserter, DirIndex, DirIndexError};

/// `canonicalize_parent_in_tx` の戻り値（観測ログ向け）
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CanonicalizeReport {
    pub cascaded_dirs: usize,
    pub removed_entries: usize,
    pub removed_meta: usize,
}

/// `name` に不正文字 (`/`, `\\0`, `\\`) が含まれていれば true
///
/// cascade range の lo/hi 構築前に必ず呼ぶ。Walker 由来の new name + 永続層から
/// 読み出した old name の両方に適用する (defense-in-depth)。
pub(crate) fn name_has_invalid_byte(name: &str) -> bool {
    name.bytes().any(|b| b == b'/' || b == 0 || b == b'\\')
}

/// 永続層破損から自動復旧する共通ヘルパ
///
/// `canonicalize_parent_in_tx` が `CorruptPersistentName` を返した場合、呼び出し側
/// (`run_mount_scan` / `browse fallback writeback`) はこのヘルパを呼ぶ。
///
/// - 別 connection + 別 tx で `delete_mount_entries(mount_id)` を実行
///   (信頼済み `mount_id` range のみ使用、不正 name は cascade に使わない)
/// - `Indexer::clear_mount_fingerprint()` で次回起動を cold start に強制
/// - ERROR ログを出力
///
/// active な `&Transaction` 内で呼んではならない (`SQLITE_BUSY` 競合)。
pub(crate) fn recover_from_corrupt_persistent_name(
    dir_index: &DirIndex,
    indexer: &Indexer,
    mount_id: &str,
    parent_path: &str,
    name: &str,
) -> Result<(), DirIndexError> {
    tracing::error!(
        mount_id,
        parent_path,
        ?name,
        "永続層の name 破損を検出: mount 全削除 + fingerprint クリアで自動復旧"
    );
    let removed = dir_index.delete_mount_entries(mount_id)?;
    if let Err(e) = indexer.clear_mount_fingerprint() {
        // fingerprint クリア失敗は致命的だが、DirIndex は既に空にできた
        tracing::error!(
            mount_id,
            error = %e,
            "fingerprint クリア失敗。次回起動も warm start になる可能性あり"
        );
    }
    tracing::info!(mount_id, removed_entries = removed, "破損リカバリ完了");
    Ok(())
}

/// 1 つの `parent_path` の子エントリを正本化 (per-parent cascade canonical replace)
///
/// active `&Transaction` 内で呼ぶ。`new_subdirs` / `new_files` を「parent の現在の真」
/// として、旧子集合を破棄しつつ消えたサブディレクトリ配下も cascade 削除する。
///
/// 処理順 (1 tx 内):
/// 1. 新 name の lexical validation (`/`, `\\0`, `\\` reject)
/// 2. 旧 dir 子集合を `SELECT` し old name にも同じ validation を適用
///    (破損検出時は `CorruptPersistentName` Err を返し、tx は呼び出し側が rollback)
/// 3. 旧 dir 集合 - 新 dir 集合 の差分を cascade DELETE
///    - half-open range `["{removed}/", "{removed}0")` で配下、`= ?removed` で自身
///    - `SQLite` default `BINARY` collation を `COLLATE BINARY` で明示
/// 4. parent の `dir_entries` 全行 DELETE (旧ファイル子と旧 dir 行を一括除去)
/// 5. 新 `subdirs` (kind='directory') + 新 `files` を `INSERT OR REPLACE`
/// 6. `dir_meta(parent_path, mtime_ns)` を `INSERT OR REPLACE`
///
/// 戻り値の `CanonicalizeReport` で観測ログを記録する。
#[allow(
    clippy::too_many_lines,
    reason = "1 tx 内で 6 step を一括実行する設計。分割すると tx 境界の保証が崩れるため一塊で読みやすい"
)]
pub(crate) fn canonicalize_parent_in_tx(
    tx: &Transaction,
    mount_id: &str,
    parent_path: &str,
    dir_mtime_ns: i64,
    new_subdirs: &[(String, i64)],
    new_files: &[(String, i64, i64)],
) -> Result<CanonicalizeReport, DirIndexError> {
    // Step 1: 新 name の lexical validation
    for (name, _) in new_subdirs {
        if name_has_invalid_byte(name) {
            return Err(DirIndexError::Other(format!(
                "new subdir name に不正文字: {name:?} (parent_path={parent_path})"
            )));
        }
    }
    for (name, _, _) in new_files {
        if name_has_invalid_byte(name) {
            return Err(DirIndexError::Other(format!(
                "new file name に不正文字: {name:?} (parent_path={parent_path})"
            )));
        }
    }

    // Step 2: 旧 dir 子集合を取得 + defense-in-depth validation
    let old_dir_names: Vec<String> = {
        let mut stmt = tx.prepare_cached(
            "SELECT name FROM dir_entries WHERE parent_path = ?1 AND kind = 'directory'",
        )?;
        let rows = stmt.query_map(params![parent_path], |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();
        for r in rows {
            let name = r?;
            if name_has_invalid_byte(&name) {
                return Err(DirIndexError::CorruptPersistentName {
                    mount_id: mount_id.to_owned(),
                    parent_path: parent_path.to_owned(),
                    name,
                });
            }
            names.push(name);
        }
        names
    };

    // Step 3: 消えた dir 子の cascade 削除
    let new_dir_set: std::collections::BTreeSet<&str> =
        new_subdirs.iter().map(|(n, _)| n.as_str()).collect();
    let mut report = CanonicalizeReport::default();

    for old_name in &old_dir_names {
        if new_dir_set.contains(old_name.as_str()) {
            continue;
        }
        let removed = format!("{parent_path}/{old_name}");
        let lo_subtree = format!("{removed}/");
        let hi_subtree = format!("{removed}0");

        // 自身の行を削除
        let removed_self = tx.execute(
            "DELETE FROM dir_entries WHERE parent_path = ?1",
            params![&removed],
        )?;
        let removed_meta_self =
            tx.execute("DELETE FROM dir_meta WHERE path = ?1", params![&removed])?;
        // 配下を半開区間で削除 (BINARY collation 明示)
        let removed_descendants = tx.execute(
            "DELETE FROM dir_entries \
             WHERE parent_path >= ?1 COLLATE BINARY \
               AND parent_path < ?2 COLLATE BINARY",
            params![&lo_subtree, &hi_subtree],
        )?;
        let removed_meta_desc = tx.execute(
            "DELETE FROM dir_meta \
             WHERE path >= ?1 COLLATE BINARY \
               AND path < ?2 COLLATE BINARY",
            params![&lo_subtree, &hi_subtree],
        )?;

        report.cascaded_dirs += 1;
        report.removed_entries += removed_self + removed_descendants;
        report.removed_meta += removed_meta_self + removed_meta_desc;
    }

    // Step 4: parent の dir_entries 全行削除
    let removed_parent_rows = tx.execute(
        "DELETE FROM dir_entries WHERE parent_path = ?1",
        params![parent_path],
    )?;
    report.removed_entries += removed_parent_rows;

    // Step 5: 新エントリを INSERT OR REPLACE
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO dir_entries \
                 (parent_path, name, kind, sort_key, size_bytes, mtime_ns) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (name, mtime_ns) in new_subdirs {
            let sort_key = encode_sort_key(name);
            stmt.execute(params![
                parent_path,
                name,
                "directory",
                sort_key,
                Option::<i64>::None,
                mtime_ns,
            ])?;
        }
        for (name, size_bytes, mtime_ns) in new_files {
            let kind = classify_kind(name);
            let sort_key = encode_sort_key(name);
            stmt.execute(params![
                parent_path,
                name,
                kind,
                sort_key,
                size_bytes,
                mtime_ns
            ])?;
        }
    }

    // Step 6: dir_meta を更新
    tx.execute(
        "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?1, ?2)",
        params![parent_path, dir_mtime_ns],
    )?;

    Ok(report)
}

impl DirIndex {
    /// `parallel_walk` の `WalkCallbackArgs` を受け取り `DirIndex` に格納する (full snapshot API)
    ///
    /// - `walk_entry_path` を `root_dir` からの相対パスに変換し `mount_id` をプレフィックス
    /// - 1 tx で `canonicalize_parent_in_tx` を呼ぶため、`entries` + `dir_meta` が原子的に
    ///   更新される (旧実装の「`entries` commit 後に `dir_meta` を別 execute」分割を解消)
    /// - `args.is_complete == false` のときは早期 return + WARN (`cascade` skip、既存行保持)
    /// - `CorruptPersistentName` を受けたら **そのまま `Err` を呼び出し側に伝播**
    ///   (リカバリは call site で `recover_from_corrupt_persistent_name` を呼ぶ)
    pub(crate) fn ingest_walk_entry(&self, args: &WalkCallbackArgs) -> Result<(), DirIndexError> {
        if !args.is_complete {
            tracing::warn!(
                mount_id = %args.mount_id,
                walk_entry_path = %args.walk_entry_path,
                "is_complete=false の WalkEntry を skip (DirIndex 既存行を保持)"
            );
            return Ok(());
        }

        let conn = self.connect()?;
        let parent_path = build_parent_path(args);

        let tx = conn.unchecked_transaction()?;
        canonicalize_parent_in_tx(
            &tx,
            &args.mount_id,
            &parent_path,
            args.dir_mtime_ns,
            &args.subdirs,
            &args.files,
        )?;
        tx.commit()?;

        Ok(())
    }

    /// ディレクトリの mtime を記録する
    pub(crate) fn set_dir_mtime(&self, path: &str, mtime_ns: i64) -> Result<(), DirIndexError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO dir_meta (path, mtime_ns) VALUES (?1, ?2)",
            params![path, mtime_ns],
        )?;
        Ok(())
    }

    /// フルスキャンが完了しているかを返す
    pub(crate) fn is_full_scan_done(&self) -> Result<bool, DirIndexError> {
        let conn = self.connect()?;
        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'full_scan_done'",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(result.as_deref() == Some("1"))
    }

    /// フルスキャン完了フラグを設定する
    pub(crate) fn mark_full_scan_done(&self) -> Result<(), DirIndexError> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('full_scan_done', '1')",
            [],
        )?;
        Ok(())
    }

    /// 指定 `mount_id` 配下の `dir_entries` + `dir_meta` 行を一括削除する
    ///
    /// - 16 桁 lowercase hex invariant は `mount_scope_range` で検証
    ///   （違反は `DirIndexError::Other` で reject、traversal 防御を継承）
    /// - range は `[mount_id, mount_id + "0")` に拡張する：
    ///   - `Indexer::entries.relative_path` は常に `{mount_id}/{path}` 形式だが、
    ///     `DirIndex` は **ルート行 `parent_path = mount_id`** も格納するため
    ///     `[{mount_id}/, ...)` では取りこぼす
    ///   - `"0"` (0x30) は `"/"` (0x2F) の直後なので、`{mount_id}` と
    ///     `{mount_id}/...` の両方を含み、16 桁固定長なら他マウント衝突なし
    /// - BINARY range scan: `dir_entries` は `idx_dir_parent`、`dir_meta` は
    ///   TEXT PRIMARY KEY の auto-index (`sqlite_autoindex_dir_meta_1`) で SEARCH
    /// - 2 テーブルを 1 tx で削除（片方失敗 → 両方ロールバック）
    /// - 返値: 削除した `dir_entries` の行数
    pub(crate) fn delete_mount_entries(&self, mount_id: &str) -> Result<usize, DirIndexError> {
        // invariant 検証のためだけに mount_scope_range を呼ぶ（hi も再利用）
        let (_, hi) = mount_scope_range(mount_id)?;
        let lo = mount_id.to_owned();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        let removed_entries = tx.execute(
            "DELETE FROM dir_entries WHERE parent_path >= ?1 AND parent_path < ?2",
            params![lo, hi],
        )?;
        tx.execute(
            "DELETE FROM dir_meta WHERE path >= ?1 AND path < ?2",
            params![lo, hi],
        )?;
        tx.commit()?;
        Ok(removed_entries)
    }

    /// バルク挿入用の `BulkInserter` を生成する
    ///
    /// 単一接続 + `synchronous=OFF` で高速に格納する。
    /// `DirIndex` はキャッシュ DB のため、中断時のデータ損失は許容。
    pub(crate) fn begin_bulk(&self) -> Result<BulkInserter, DirIndexError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;\
             PRAGMA busy_timeout=5000;\
             PRAGMA synchronous=OFF;\
             PRAGMA cache_size=-16384;\
             PRAGMA temp_store=MEMORY;",
        )?;
        Ok(BulkInserter::new(conn))
    }
}
