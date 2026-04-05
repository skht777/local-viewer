//! `node_id` ↔ 実パス マッピング管理
//!
//! `node_id` は `HMAC-SHA256(secret, relative_path)` の先頭16文字 (hex)。
//! - 同じパスに対して常に同じ `node_id` を返す (冪等)
//! - secret により外部からの推測を防止
//! - クライアントに実パスを公開しない

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::errors::AppError;
use crate::services::path_security::PathSecurity;

type HmacSha256 = Hmac<Sha256>;

/// `node_id` ↔ 実パスのマッピングを管理する
///
/// - HMAC-SHA256 でパスから `node_id` を決定的に生成
/// - 双方向マッピングをメモリに保持
/// - `register()` は内部で `resolve()` を呼ぶが `validate()` は呼ばない
/// - `_generate_id()` 内の `find_root_for()` がルート外パスを拒否する最終防壁
pub(crate) struct NodeRegistry {
    path_security: Arc<PathSecurity>,
    secret: Vec<u8>,
    id_to_path: HashMap<String, PathBuf>,
    path_to_id: HashMap<String, String>,
    // 文字列比較用キャッシュ (root_str, root_prefix, root)
    root_entries: Vec<(String, String, PathBuf)>,
    // マウントポイント名マッピング
    mount_names: HashMap<PathBuf, String>,
    // アーカイブエントリ用 LRU
    id_to_archive_entry: HashMap<String, (PathBuf, String)>,
    archive_entry_to_id: HashMap<String, String>,
    id_to_composite_key: HashMap<String, String>,
    archive_order: VecDeque<String>,
    archive_registry_max: usize,
}

impl NodeRegistry {
    /// 新規作成
    pub(crate) fn new(
        path_security: Arc<PathSecurity>,
        archive_registry_max_entries: usize,
        mount_names: HashMap<PathBuf, String>,
    ) -> Self {
        let root_entries = path_security
            .root_dirs()
            .iter()
            .map(|r| {
                let s = r.to_string_lossy().to_string();
                let prefix = format!("{s}{}", std::path::MAIN_SEPARATOR);
                (s, prefix, r.clone())
            })
            .collect();

        let secret = std::env::var("NODE_SECRET")
            .unwrap_or_else(|_| "local-viewer-default-secret".to_string())
            .into_bytes();

        Self {
            path_security,
            secret,
            id_to_path: HashMap::new(),
            path_to_id: HashMap::new(),
            root_entries,
            mount_names,
            id_to_archive_entry: HashMap::new(),
            archive_entry_to_id: HashMap::new(),
            id_to_composite_key: HashMap::new(),
            archive_order: VecDeque::new(),
            archive_registry_max: archive_registry_max_entries,
        }
    }

    /// テスト用: secret を明示的に指定して作成
    #[cfg(test)]
    fn with_secret(
        path_security: Arc<PathSecurity>,
        secret: &[u8],
        mount_names: HashMap<PathBuf, String>,
    ) -> Self {
        let root_entries = path_security
            .root_dirs()
            .iter()
            .map(|r| {
                let s = r.to_string_lossy().to_string();
                let prefix = format!("{s}{}", std::path::MAIN_SEPARATOR);
                (s, prefix, r.clone())
            })
            .collect();

        Self {
            path_security,
            secret: secret.to_vec(),
            id_to_path: HashMap::new(),
            path_to_id: HashMap::new(),
            root_entries,
            mount_names,
            id_to_archive_entry: HashMap::new(),
            archive_entry_to_id: HashMap::new(),
            id_to_composite_key: HashMap::new(),
            archive_order: VecDeque::new(),
            archive_registry_max: 100_000,
        }
    }

    pub(crate) fn path_security(&self) -> &PathSecurity {
        &self.path_security
    }

    /// パスから決定的な `node_id` を生成する (内部用)
    ///
    /// `HMAC-SHA256(secret, "{root}::{relative_path}")` の先頭16文字。
    /// ルートパスを入力に含め、異なるマウントの同名ファイルの衝突を回避。
    fn generate_id(&self, path: &Path) -> Result<String, AppError> {
        let root = self.path_security.find_root_for(path).ok_or_else(|| {
            AppError::path_security(format!(
                "パスがどのルートにも属しません: {}",
                path.display()
            ))
        })?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| AppError::path_security("相対パスの取得に失敗"))?;
        let hmac_input = format!(
            "{root}::{relative}",
            root = root.display(),
            relative = relative.display()
        );
        Ok(self.hmac_hex(&hmac_input))
    }

    /// パスを登録し、`node_id` を返す
    ///
    /// 既に登録済みならキャッシュから返す。
    /// 外部からの呼び出し用。`resolve()` で正規化する (fail-closed)。
    pub(crate) fn register(&mut self, path: &Path) -> Result<String, AppError> {
        let resolved = std::fs::canonicalize(path).map_err(|_| {
            AppError::path_security(format!("パスの解決に失敗: {}", path.display()))
        })?;
        let key = resolved.to_string_lossy().to_string();
        if let Some(id) = self.path_to_id.get(&key) {
            return Ok(id.clone());
        }

        let node_id = self.generate_id(&resolved)?;
        self.id_to_path.insert(node_id.clone(), resolved);
        self.path_to_id.insert(key, node_id.clone());
        Ok(node_id)
    }

    /// 検証済み・正規化済みパスを登録する (内部用 fast-path)
    ///
    /// `validate` / `validate_child` 済みのパスのみ渡すこと。
    /// `resolve()` と `relative_to()` をスキップして高速化。
    pub(crate) fn register_resolved(&mut self, resolved: &Path) -> String {
        let key = resolved.to_string_lossy().to_string();
        if let Some(id) = self.path_to_id.get(&key) {
            return id.clone();
        }

        // 文字列スライスで相対パス取得 (root 配下が保証済み)
        let mut root_str = "";
        let mut rel = "";
        for (rs, rp, _) in &self.root_entries {
            if key == *rs {
                root_str = rs;
                rel = "";
                break;
            }
            if key.starts_with(rp.as_str()) {
                root_str = rs;
                rel = &key[rp.len()..];
                break;
            }
        }

        let hmac_input = format!("{root_str}::{rel}");
        let node_id = self.hmac_hex(&hmac_input);
        self.id_to_path
            .insert(node_id.clone(), resolved.to_path_buf());
        self.path_to_id.insert(key, node_id.clone());
        node_id
    }

    /// `node_id` から実パスを返す
    pub(crate) fn resolve(&self, node_id: &str) -> Result<&Path, AppError> {
        self.id_to_path
            .get(node_id)
            .map(PathBuf::as_path)
            .ok_or_else(|| AppError::node_not_found(node_id))
    }

    /// パスの親ディレクトリの `node_id` を返す
    ///
    /// ルートディレクトリ自体の場合のみ `None` を返す。
    pub(crate) fn get_parent_node_id(&mut self, path: &Path) -> Option<String> {
        let resolved = std::fs::canonicalize(path).ok()?;
        let roots = self.path_security.root_dirs();
        if roots.contains(&resolved) {
            return None;
        }
        let parent = resolved.parent()?;
        self.path_security.validate(parent).ok()?;
        self.register(parent).ok()
    }

    /// パスの祖先エントリを返す (マウントルートから親まで)
    ///
    /// パンくずリスト表示用。現在のディレクトリ自体は含まない。
    pub(crate) fn get_ancestors(&mut self, path: &Path) -> Vec<(String, String)> {
        let Ok(resolved) = std::fs::canonicalize(path) else {
            return vec![];
        };
        let Some(root) = self.path_security.find_root_for(&resolved) else {
            return vec![];
        };
        let root = root.to_path_buf();
        if resolved == root {
            return vec![];
        }

        let mut ancestors: Vec<(String, String)> = Vec::new();
        let mut current = resolved.parent().map(Path::to_path_buf);
        while let Some(ref cur) = current {
            if *cur == root {
                break;
            }
            let node_id = self.register_resolved(cur);
            let name = cur.file_name().map_or_else(
                || cur.to_string_lossy().to_string(),
                |n| n.to_string_lossy().to_string(),
            );
            ancestors.push((node_id, name));
            current = cur.parent().map(Path::to_path_buf);
        }

        // マウントルート自体を追加
        let root_node_id = self.register_resolved(&root);
        let root_name = self.mount_names.get(&root).cloned().unwrap_or_else(|| {
            root.file_name().map_or_else(
                || root.to_string_lossy().to_string(),
                |n| n.to_string_lossy().to_string(),
            )
        });
        ancestors.push((root_node_id, root_name));

        ancestors.reverse();
        ancestors
    }

    // --- アーカイブエントリ対応 ---

    /// アーカイブエントリを登録し `node_id` を返す
    ///
    /// HMAC 入力: `"arc::{root}::{archive_relative}::{entry_name}"`
    /// LRU 方式で上限超過時は最も古い登録を削除。
    pub(crate) fn register_archive_entry(
        &mut self,
        archive_path: &Path,
        entry_name: &str,
    ) -> Result<String, AppError> {
        let composite_key = format!(
            "arc::{archive_path}::{entry_name}",
            archive_path = archive_path.display()
        );
        if let Some(id) = self.archive_entry_to_id.get(&composite_key) {
            // LRU: move to end
            let id_clone = id.clone();
            if let Some(pos) = self.archive_order.iter().position(|x| *x == id_clone) {
                self.archive_order.remove(pos);
            }
            self.archive_order.push_back(id_clone.clone());
            return Ok(id_clone);
        }

        // HMAC でアーカイブ相対パスとエントリ名から node_id を生成
        let resolved = std::fs::canonicalize(archive_path).map_err(|_| {
            AppError::path_security(format!(
                "アーカイブパスの解決に失敗: {}",
                archive_path.display()
            ))
        })?;
        let root = self
            .path_security
            .find_root_for(&resolved)
            .ok_or_else(|| AppError::path_security("アーカイブがどのルートにも属しません"))?;
        let rel = resolved
            .strip_prefix(root)
            .map_err(|_| AppError::path_security("相対パスの取得に失敗"))?;
        let hmac_input = format!(
            "arc::{root}::{rel}::{entry_name}",
            root = root.display(),
            rel = rel.display()
        );
        let node_id = self.hmac_hex(&hmac_input);

        // LRU 上限管理
        while self.id_to_archive_entry.len() >= self.archive_registry_max {
            if let Some(evicted_id) = self.archive_order.pop_front() {
                self.id_to_archive_entry.remove(&evicted_id);
                if let Some(evicted_key) = self.id_to_composite_key.remove(&evicted_id) {
                    self.archive_entry_to_id.remove(&evicted_key);
                }
            } else {
                break;
            }
        }

        self.id_to_archive_entry
            .insert(node_id.clone(), (resolved, entry_name.to_string()));
        self.archive_entry_to_id
            .insert(composite_key.clone(), node_id.clone());
        self.id_to_composite_key
            .insert(node_id.clone(), composite_key);
        self.archive_order.push_back(node_id.clone());
        Ok(node_id)
    }

    /// `node_id` がアーカイブエントリなら `(archive_path, entry_name)` を返す
    pub(crate) fn resolve_archive_entry(&mut self, node_id: &str) -> Option<(PathBuf, String)> {
        let result = self.id_to_archive_entry.get(node_id)?.clone();
        // LRU: move to end
        if let Some(pos) = self.archive_order.iter().position(|x| x == node_id) {
            self.archive_order.remove(pos);
        }
        self.archive_order.push_back(node_id.to_string());
        Some(result)
    }

    /// `node_id` がアーカイブエントリかどうか
    pub(crate) fn is_archive_entry(&self, node_id: &str) -> bool {
        self.id_to_archive_entry.contains_key(node_id)
    }

    /// HMAC-SHA256 の先頭 16 hex 文字を返す
    fn hmac_hex(&self, input: &str) -> String {
        #[allow(
            clippy::expect_used,
            reason = "HMAC-SHA256 は任意長の鍵を受け付ける (Sha256 には鍵長制限なし)"
        )]
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC は任意長の鍵を受け付ける");
        mac.update(input.as_bytes());
        let result = mac.finalize().into_bytes();
        hex::encode(result)[..16].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use tempfile::TempDir;

    const TEST_SECRET: &[u8] = b"local-viewer-default-secret";

    struct TestEnv {
        #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
        dir: TempDir,
        root: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            fs::write(root.join("file.txt"), "hello").unwrap();
            fs::create_dir_all(root.join("subdir")).unwrap();
            fs::write(root.join("subdir/nested.txt"), "nested").unwrap();
            Self { dir, root }
        }

        fn registry(&self) -> NodeRegistry {
            let ps = Arc::new(PathSecurity::new(vec![self.root.clone()], false).unwrap());
            NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new())
        }
    }

    // --- 基本 register / resolve ---

    #[test]
    fn パスを登録してnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let id = reg.register(&env.root.join("file.txt")).unwrap();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn 同じパスに対して同じnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let id1 = reg.register(&env.root.join("file.txt")).unwrap();
        let id2 = reg.register(&env.root.join("file.txt")).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn 異なるパスに対して異なるnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let id1 = reg.register(&env.root.join("file.txt")).unwrap();
        let id2 = reg.register(&env.root.join("subdir/nested.txt")).unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn node_idから元のパスを解決する() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let file_path = env.root.join("file.txt");
        let id = reg.register(&file_path).unwrap();
        let resolved = reg.resolve(&id).unwrap();
        assert_eq!(resolved, fs::canonicalize(&file_path).unwrap());
    }

    #[test]
    fn 未登録のnode_idでnot_foundエラー() {
        let env = TestEnv::new();
        let reg = env.registry();
        let err = reg.resolve("nonexistent").unwrap_err();
        assert!(err.to_string().contains("見つかりません"));
    }

    // --- register_resolved ---

    #[test]
    fn register_resolvedがregisterと同じnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let resolved = fs::canonicalize(env.root.join("file.txt")).unwrap();
        let id1 = reg.register(&env.root.join("file.txt")).unwrap();
        let id2 = reg.register_resolved(&resolved);
        assert_eq!(id1, id2);
    }

    // --- get_parent_node_id ---

    #[test]
    fn 親のnode_idが取得できる() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let parent_id = reg.get_parent_node_id(&env.root.join("subdir/nested.txt"));
        assert!(parent_id.is_some());
    }

    #[test]
    fn root_dirの親はnone() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let parent_id = reg.get_parent_node_id(&env.root);
        assert!(parent_id.is_none());
    }

    // --- get_ancestors ---

    #[test]
    fn ルートディレクトリのancestorsが空リストを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root);
        assert!(ancestors.is_empty());
    }

    #[test]
    fn ルート直下ディレクトリのancestorsがルートのみを含む() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root.join("subdir"));
        assert_eq!(ancestors.len(), 1);
    }

    #[test]
    fn 深い階層のancestorsが全祖先を含む() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root.join("subdir/nested.txt"));
        // ルート + subdir = 2 件
        assert_eq!(ancestors.len(), 2);
    }

    #[test]
    fn ancestorsの順序がルートから親へ正しい() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let ancestors = reg.get_ancestors(&env.root.join("subdir/nested.txt"));
        // 先頭がルート
        assert_eq!(
            ancestors[0].1,
            env.root.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(ancestors[1].1, "subdir");
    }

    #[test]
    fn ancestorsのルートエントリ名がmount_namesを反映する() {
        let env = TestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let mut names = HashMap::new();
        let canonical_root = fs::canonicalize(&env.root).unwrap();
        names.insert(canonical_root, "My Pictures".to_string());
        let mut reg = NodeRegistry::with_secret(ps, TEST_SECRET, names);
        let ancestors = reg.get_ancestors(&env.root.join("subdir"));
        assert_eq!(ancestors[0].1, "My Pictures");
    }

    // --- アーカイブエントリ ---

    #[test]
    fn アーカイブエントリを登録してnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        // アーカイブファイルを作成 (テスト用)
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let id = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn 同じアーカイブエントリに対して同じnode_idを返す() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let id1 = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        let id2 = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn アーカイブエントリのnode_idを解決できる() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let id = reg.register_archive_entry(&archive, "page01.jpg").unwrap();
        let (path, entry) = reg.resolve_archive_entry(&id).unwrap();
        assert_eq!(path, fs::canonicalize(&archive).unwrap());
        assert_eq!(entry, "page01.jpg");
    }

    #[test]
    fn is_archive_entryが正しく判定する() {
        let env = TestEnv::new();
        let mut reg = env.registry();
        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();
        let arc_id = reg.register_archive_entry(&archive, "p.jpg").unwrap();
        let file_id = reg.register(&env.root.join("file.txt")).unwrap();
        assert!(reg.is_archive_entry(&arc_id));
        assert!(!reg.is_archive_entry(&file_id));
    }

    #[test]
    fn アーカイブエントリ上限超過で最古エントリがevictされる() {
        let env = TestEnv::new();
        let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
        let mut reg = NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new());
        reg.archive_registry_max = 2;

        let archive = env.root.join("test.zip");
        fs::write(&archive, "fake zip").unwrap();

        let id1 = reg.register_archive_entry(&archive, "p1.jpg").unwrap();
        let _id2 = reg.register_archive_entry(&archive, "p2.jpg").unwrap();
        // 3 番目の登録で id1 が evict される
        let _id3 = reg.register_archive_entry(&archive, "p3.jpg").unwrap();

        assert!(!reg.is_archive_entry(&id1));
    }

    // --- HMAC ゴールデンベクターテスト ---

    /// HMAC 入力文字列と期待される `node_id` を直接テストするヘルパー
    fn compute_hmac(input: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(TEST_SECRET).expect("HMAC は任意長の鍵を受け付ける");
        mac.update(input.as_bytes());
        let result = mac.finalize().into_bytes();
        hex::encode(result)[..16].to_string()
    }

    #[test]
    fn hmac_通常ファイルのゴールデンベクター() {
        // Python で生成済みベクター (secret = b"local-viewer-default-secret")
        assert_eq!(
            compute_hmac("/mnt/data::photos/img001.jpg"),
            "cc420505916e01d4"
        );
        assert_eq!(compute_hmac("/mnt/data::"), "0b27cf020d2e8dff");
        assert_eq!(
            compute_hmac("/mnt/data::subdir/deep/file.png"),
            "ae53c4d5e3a72c78"
        );
        assert_eq!(
            compute_hmac("/mnt/data::日本語ファイル.jpg"),
            "b143bc31a26f1350"
        );
        assert_eq!(
            compute_hmac("/mnt/data::file with spaces.jpg"),
            "7acc8ddcceec554e"
        );
        assert_eq!(
            compute_hmac("/mnt/archive::zips/images.zip"),
            "299db1b9e7104f0e"
        );
    }

    #[test]
    fn hmac_アーカイブエントリのゴールデンベクター() {
        assert_eq!(
            compute_hmac("arc::/mnt/data::archive.zip::page01.jpg"),
            "fdc8fc764a07d9e9"
        );
        assert_eq!(
            compute_hmac("arc::/mnt/data::nested/comic.cbz::img/001.png"),
            "bb2c42b499b6d6f2"
        );
        assert_eq!(
            compute_hmac("arc::/mnt/data::test.zip::日本語/画像.jpg"),
            "27a6131445f16976"
        );
    }
}
