use super::scan::scan_child_meta;
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
    let id2 = reg.register_resolved(&resolved).unwrap();
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

// --- list_directory ---

struct ListTestEnv {
    #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
    dir: TempDir,
    root: PathBuf,
}

impl ListTestEnv {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        // ファイル
        fs::write(root.join("image1.jpg"), "jpg").unwrap();
        fs::write(root.join("image2.png"), "png").unwrap();
        fs::write(root.join("video.mp4"), "mp4").unwrap();
        fs::write(root.join("doc.pdf"), "pdf").unwrap();
        fs::write(root.join("readme.txt"), "txt").unwrap();
        // サブディレクトリ (画像入り)
        fs::create_dir_all(root.join("subdir")).unwrap();
        fs::write(root.join("subdir/inner.jpg"), "inner").unwrap();
        fs::write(root.join("subdir/inner2.png"), "inner2").unwrap();
        // 空ディレクトリ
        fs::create_dir_all(root.join("empty")).unwrap();
        Self { dir, root }
    }

    fn registry(&self) -> NodeRegistry {
        let ps = Arc::new(PathSecurity::new(vec![self.root.clone()], false).unwrap());
        NodeRegistry::with_secret(ps, TEST_SECRET, HashMap::new())
    }
}

#[test]
fn list_directoryが全エントリを返す() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let entries = reg.list_directory(&env.root).unwrap();
    // image1.jpg, image2.png, video.mp4, doc.pdf, readme.txt, subdir, empty = 7
    assert_eq!(entries.len(), 7);
}

#[test]
fn list_directoryでファイルが正しくclassifyされる() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let entries = reg.list_directory(&env.root).unwrap();
    let image_count = entries
        .iter()
        .filter(|e| e.kind == EntryKind::Image)
        .count();
    assert_eq!(image_count, 2);
    let video_count = entries
        .iter()
        .filter(|e| e.kind == EntryKind::Video)
        .count();
    assert_eq!(video_count, 1);
    let dir_count = entries
        .iter()
        .filter(|e| e.kind == EntryKind::Directory)
        .count();
    assert_eq!(dir_count, 2);
}

#[test]
fn ディレクトリのchild_countが正しい() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let entries = reg.list_directory(&env.root).unwrap();
    let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
    assert_eq!(subdir.child_count, Some(2)); // inner.jpg, inner2.png
}

#[test]
fn preview_node_idsが画像を含む() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let entries = reg.list_directory(&env.root).unwrap();
    let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
    let previews = subdir.preview_node_ids.as_ref().unwrap();
    assert!(!previews.is_empty());
    assert!(previews.len() <= 3);
}

#[test]
fn 空ディレクトリのpreview_node_idsがnone() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let entries = reg.list_directory(&env.root).unwrap();
    let empty = entries.iter().find(|e| e.name == "empty").unwrap();
    assert_eq!(empty.child_count, Some(0));
    assert!(empty.preview_node_ids.is_none());
}

#[test]
fn modified_atがposix秒で設定される() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let entries = reg.list_directory(&env.root).unwrap();
    let file = entries.iter().find(|e| e.name == "image1.jpg").unwrap();
    assert!(file.modified_at.is_some());
    // 2020年以降の値であること (POSIX 秒)
    assert!(file.modified_at.unwrap() > 1_577_836_800.0);
}

#[test]
fn 空ディレクトリのlist_directoryが空リストを返す() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let entries = reg.list_directory(&env.root.join("empty")).unwrap();
    assert!(entries.is_empty());
}

// --- list_directory_page ---

#[test]
fn list_directory_pageでlimit件分のみ返す() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let opts = PageOptions {
        limit: 3,
        cursor_node_id: None,
        reverse: false,
    };
    let (entries, total) = reg.list_directory_page(&env.root, &opts).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(total, 7);
}

#[test]
fn list_directory_pageの合計件数が全エントリ数() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let opts = PageOptions {
        limit: 100,
        cursor_node_id: None,
        reverse: false,
    };
    let (entries, total) = reg.list_directory_page(&env.root, &opts).unwrap();
    assert_eq!(entries.len(), 7);
    assert_eq!(total, 7);
}

// --- list_mount_roots ---

#[test]
fn list_mount_rootsが全ルートを返す() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let roots = reg.list_mount_roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].kind, EntryKind::Directory);
}

#[test]
fn list_mount_rootsのnameがmount_namesを反映() {
    let env = ListTestEnv::new();
    let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
    let mut names = HashMap::new();
    names.insert(env.root.clone(), "My Pictures".to_string());
    let mut reg = NodeRegistry::with_secret(ps, TEST_SECRET, names);
    let roots = reg.list_mount_roots();
    assert_eq!(roots[0].name, "My Pictures");
}

#[test]
fn list_mount_rootsにchild_countが含まれる() {
    let env = ListTestEnv::new();
    let mut reg = env.registry();
    let roots = reg.list_mount_roots();
    assert!(roots[0].child_count.is_some());
    assert_eq!(roots[0].child_count, Some(7));
}

// --- HMAC ゴールデンベクターテスト ---

/// HMAC 入力文字列と期待される `node_id` を直接テストするヘルパー
fn compute_hmac(input: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(TEST_SECRET).expect("HMAC は任意長の鍵を受け付ける");
    mac.update(input.as_bytes());
    let result = mac.finalize().into_bytes();
    let mut h = hex::encode(result);
    h.truncate(16);
    h
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

// --- register_resolved root ガード ---

#[test]
fn register_resolvedがルート外パスでエラーを返す() {
    let env = TestEnv::new();
    let mut reg = env.registry();
    let err = reg.register_resolved(Path::new("/nonexistent/path"));
    assert!(err.is_err());
}

// --- Two-Phase free functions ---

#[test]
fn scan_entriesがディレクトリ内エントリを返す() {
    let env = ListTestEnv::new();
    let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
    let entries = scan_entries(&ps, &env.root).unwrap();
    assert_eq!(entries.len(), 7);
}

#[test]
fn scan_child_metaが子エントリ数とプレビューパスを返す() {
    let env = ListTestEnv::new();
    let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
    let cm = scan_child_meta(&ps, &env.root.join("subdir"), 3);
    assert_eq!(cm.count, 2); // inner.jpg, inner2.png
    assert!(!cm.preview_paths.is_empty());
    assert!(cm.preview_paths.len() <= 3);
}

#[test]
fn scan_entry_metasがcanonicalize済みパスを持つ() {
    let env = ListTestEnv::new();
    let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());
    let raw = scan_entries(&ps, &env.root).unwrap();
    let stated = stat_entries(&raw);
    let scanned = scan_entry_metas(&ps, stated, 3);
    assert_eq!(scanned.len(), 7);
    // 全パスが絶対パス
    assert!(scanned.iter().all(|s| s.path.is_absolute()));
}

#[test]
fn register_scanned_entriesがlist_directoryと同じ結果を返す() {
    let env = ListTestEnv::new();
    let ps = Arc::new(PathSecurity::new(vec![env.root.clone()], false).unwrap());

    // Two-Phase パス
    let raw = scan_entries(&ps, &env.root).unwrap();
    let stated = stat_entries(&raw);
    let scanned = scan_entry_metas(&ps, stated, 3);
    let mut reg1 = env.registry();
    let two_phase = reg1.register_scanned_entries(scanned).unwrap();

    // 既存パス
    let mut reg2 = env.registry();
    let legacy = reg2.list_directory(&env.root).unwrap();

    // エントリ数が一致
    assert_eq!(two_phase.len(), legacy.len());
    // 全 node_id が一致 (順序はファイルシステム依存なので名前でソート)
    let mut tp_ids: Vec<_> = two_phase
        .iter()
        .map(|e| (e.name.as_str(), e.node_id.as_str()))
        .collect();
    let mut lg_ids: Vec<_> = legacy
        .iter()
        .map(|e| (e.name.as_str(), e.node_id.as_str()))
        .collect();
    tp_ids.sort_by_key(|(n, _)| *n);
    lg_ids.sort_by_key(|(n, _)| *n);
    assert_eq!(tp_ids, lg_ids);
}

// --- get_ancestors_from_resolved ---

#[test]
fn get_ancestors_from_resolvedがget_ancestorsと同じ結果を返す() {
    let env = TestEnv::new();
    let mut reg1 = env.registry();
    let mut reg2 = env.registry();
    let subdir = fs::canonicalize(env.root.join("subdir/nested.txt")).unwrap();
    let anc1 = reg1.get_ancestors(&subdir);
    let anc2 = reg2.get_ancestors_from_resolved(&subdir);
    assert_eq!(anc1, anc2);
}
