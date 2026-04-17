//! 起動時に Indexer 永続エントリから `NodeRegistry` を rehydrate する
//!
//! - `{mount_id}/{rest}` 形式の `relative_path` を `mount_id_map` で root 復元
//! - HMAC 冪等性（同じ secret + root + rel で同じ `node_id`）を利用し
//!   再起動前の `node_id` を再現する
//! - 永続層は信頼境界外として扱い、lexical validation で `../` 等を reject
//! - 復元結果の統計は `PopulateStats` として呼び出し側へ返す

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use tracing::warn;

use super::NodeRegistry;

/// `populate_registry` の結果統計
///
/// 運用・テスト両方から参照されるため全フィールド pub。
/// `degraded` は「個別エラーではなく populate 全体が意図的に skip された」状態を示す。
#[derive(Debug, Default, Clone)]
pub(crate) struct PopulateStats {
    pub registered: usize,
    pub skipped_missing_mount: usize,
    pub skipped_malformed: usize,
    pub skipped_traversal: usize,
    pub errors: usize,
    pub degraded: bool,
}

/// Indexer から読み出した `relative_path` 列 (`{mount_id}/{rest}` 形式) を
/// `NodeRegistry.id_to_path` に登録する
///
/// - `rest` は `Path::components()` で `Component::Normal` のみ許可（`../`, 絶対パス等を reject）
/// - `mount_id_map` に該当キーがない行は `skipped_missing_mount`
/// - `/` を含まない行は `skipped_malformed`
/// - `register_resolved` が Err を返した行は `errors`
pub(crate) fn populate_registry(
    registry: &mut NodeRegistry,
    paths: &[String],
    mount_id_map: &HashMap<String, PathBuf>,
) -> PopulateStats {
    let mut stats = PopulateStats::default();

    for raw in paths {
        let Some((mount_id, rest)) = raw.split_once('/') else {
            stats.skipped_malformed += 1;
            continue;
        };

        if rest.is_empty() {
            stats.skipped_malformed += 1;
            continue;
        }

        if !is_safe_rest(rest) {
            stats.skipped_traversal += 1;
            continue;
        }

        let Some(root_abs) = mount_id_map.get(mount_id) else {
            stats.skipped_missing_mount += 1;
            continue;
        };

        let abs = root_abs.join(rest);
        match registry.register_resolved(&abs) {
            Ok(_) => stats.registered += 1,
            Err(e) => {
                warn!(
                    path = %abs.display(),
                    "populate: register_resolved 失敗: {e}"
                );
                stats.errors += 1;
            }
        }
    }

    stats
}

/// `rest` が `Path::components()` で `Component::Normal` のみからなるかを検証する
///
/// - NUL バイト混入は reject
/// - `../`, 先頭 `/`, Windows ドライブプレフィックスを reject
/// - 空成分（連続スラッシュ等）も `Component::Normal` ではないため reject
fn is_safe_rest(rest: &str) -> bool {
    if rest.as_bytes().contains(&0) {
        return false;
    }
    Path::new(rest)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::path_security::PathSecurity;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    const TEST_SECRET: &[u8] = b"local-viewer-default-secret";

    struct Env {
        #[allow(dead_code, reason = "TempDir 保持のため")]
        dir: TempDir,
        root: PathBuf,
    }

    impl Env {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            fs::create_dir_all(root.join("subdir")).unwrap();
            fs::write(root.join("subdir/file.txt"), "x").unwrap();
            Self { dir, root }
        }

        fn registry_and_map(&self) -> (NodeRegistry, HashMap<String, PathBuf>) {
            let ps = Arc::new(PathSecurity::new(vec![self.root.clone()], false).unwrap());
            let reg = NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new());
            let mut map = HashMap::new();
            map.insert("mount_a".to_string(), self.root.clone());
            (reg, map)
        }
    }

    #[test]
    fn 正常登録でregister_resolvedと同じnode_idが生成される() {
        let env = Env::new();
        let (mut reg, map) = env.registry_and_map();

        // populate 経由で登録
        let stats = populate_registry(&mut reg, &["mount_a/subdir/file.txt".to_string()], &map);
        assert_eq!(stats.registered, 1);
        assert_eq!(stats.errors, 0);

        // 直接 register_resolved で同じパスを登録 → HMAC 冪等性で同じ node_id
        let mut reg2 = {
            let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
            NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new())
        };
        let abs = env.root.join("subdir/file.txt");
        let id_direct = reg2.register_resolved(&abs).unwrap();

        let id_populated = reg
            .path_to_id_get(&abs.to_string_lossy())
            .expect("populate で登録済み");
        assert_eq!(id_direct, id_populated);
    }

    #[test]
    fn mount欠落エントリはskipped_missing_mount() {
        let env = Env::new();
        let (mut reg, map) = env.registry_and_map();
        let stats = populate_registry(
            &mut reg,
            &["mount_missing/subdir/file.txt".to_string()],
            &map,
        );
        assert_eq!(stats.registered, 0);
        assert_eq!(stats.skipped_missing_mount, 1);
    }

    #[test]
    fn スラッシュを含まないエントリはskipped_malformed() {
        let env = Env::new();
        let (mut reg, map) = env.registry_and_map();
        let stats = populate_registry(&mut reg, &["no_slash".to_string()], &map);
        assert_eq!(stats.skipped_malformed, 1);
        assert_eq!(stats.registered, 0);
    }

    #[test]
    fn parent_dir含みrestはskipped_traversal() {
        let env = Env::new();
        let (mut reg, map) = env.registry_and_map();
        let stats = populate_registry(&mut reg, &["mount_a/../outside/evil".to_string()], &map);
        assert_eq!(stats.skipped_traversal, 1);
        assert_eq!(stats.registered, 0);
    }

    #[test]
    fn 絶対パス先頭スラッシュはskipped_traversal() {
        let env = Env::new();
        let (mut reg, map) = env.registry_and_map();
        let stats = populate_registry(&mut reg, &["mount_a//absolute".to_string()], &map);
        assert_eq!(stats.skipped_traversal, 1);
        assert_eq!(stats.registered, 0);
    }

    #[test]
    fn 空restはskipped_malformed() {
        let env = Env::new();
        let (mut reg, map) = env.registry_and_map();
        let stats = populate_registry(&mut reg, &["mount_a/".to_string()], &map);
        assert_eq!(stats.skipped_malformed, 1);
        assert_eq!(stats.registered, 0);
    }

    #[test]
    fn nulバイト含みrestはskipped_traversal() {
        let env = Env::new();
        let (mut reg, map) = env.registry_and_map();
        let stats = populate_registry(&mut reg, &["mount_a/sub\0dir/file".to_string()], &map);
        assert_eq!(stats.skipped_traversal, 1);
        assert_eq!(stats.registered, 0);
    }
}
