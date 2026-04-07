//! パストラバーサル防止モジュール
//!
//! 全ファイルアクセスはこのモジュールを経由する。
//! - `canonicalize()` 後に許可ルートディレクトリ配下であることを検証
//! - symlink はデフォルトで追跡しない
//! - 不正アクセスは `AppError::PathSecurity` を送出

use std::path::{Path, PathBuf};

use crate::errors::AppError;

/// パスの安全性を検証するサービス
///
/// - `roots`: 許可されたルートディレクトリのリスト
/// - `is_allow_symlinks`: symlink 追跡の許可フラグ
#[derive(Debug)]
pub(crate) struct PathSecurity {
    roots: Vec<PathBuf>,
    // 文字列比較用キャッシュ (root_str, root_prefix, root)
    root_entries: Vec<(String, String, PathBuf)>,
    is_allow_symlinks: bool,
}

impl PathSecurity {
    /// 複数ルートで初期化する
    ///
    /// 最低 1 つのルートが必要。各ルートは `canonicalize()` で正規化される。
    pub(crate) fn new(root_dirs: Vec<PathBuf>, is_allow_symlinks: bool) -> Result<Self, AppError> {
        if root_dirs.is_empty() {
            return Err(AppError::path_security("root_dirs は少なくとも1つ必要です"));
        }

        let roots: Vec<PathBuf> = root_dirs
            .into_iter()
            .map(|r| {
                std::fs::canonicalize(&r).map_err(|_| {
                    AppError::path_security(format!(
                        "ルートディレクトリの解決に失敗: {}",
                        r.display()
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let root_entries = roots
            .iter()
            .map(|r| {
                let s = r.to_string_lossy().to_string();
                let prefix = format!("{s}{}", std::path::MAIN_SEPARATOR);
                (s, prefix, r.clone())
            })
            .collect();

        Ok(Self {
            roots,
            root_entries,
            is_allow_symlinks,
        })
    }

    /// 全許可ルートディレクトリを返す
    pub(crate) fn root_dirs(&self) -> &[PathBuf] {
        &self.roots
    }

    /// 文字列比較用キャッシュ (`root_str`, `root_prefix`, root) を返す
    pub(crate) fn root_entries(&self) -> &[(String, String, PathBuf)] {
        &self.root_entries
    }

    /// パスが属するルートディレクトリを返す
    ///
    /// どのルートにも属さなければ `None`。
    /// `resolved` は `canonicalize()` 済みであること。
    pub(crate) fn find_root_for(&self, resolved: &Path) -> Option<&Path> {
        let s = resolved.to_string_lossy();
        for (root_str, root_prefix, root) in &self.root_entries {
            if *s == *root_str || s.starts_with(root_prefix.as_str()) {
                return Some(root);
            }
        }
        None
    }

    /// パスを検証し、解決済みの安全なパスを返す
    ///
    /// 検証手順:
    /// 1. `canonicalize()` で正規化
    /// 2. 許可ルートディレクトリのいずれか配下であることを確認
    /// 3. symlink チェック (許可されていない場合)
    pub(crate) fn validate(&self, path: &Path) -> Result<PathBuf, AppError> {
        let resolved = resolve_path(path)?;

        if !self.is_under_root(&resolved) {
            return Err(AppError::path_security(
                "許可ルートディレクトリの外へのアクセスは禁止されています",
            ));
        }

        if !self.is_allow_symlinks && self.has_symlink(path) {
            return Err(AppError::path_security(
                "symlink の追跡は許可されていません",
            ));
        }

        Ok(resolved)
    }

    /// パスを検証し、存在することも確認する
    pub(crate) fn validate_existing(&self, path: &Path) -> Result<PathBuf, AppError> {
        let resolved = self.validate(path)?;
        if !resolved.exists() {
            return Err(AppError::FileNotFound {
                path: resolved.to_string_lossy().to_string(),
            });
        }
        Ok(resolved)
    }

    /// 先頭ルートと部分パスを安全に結合する
    ///
    /// 各部分パスに不正な要素がないか検証してから結合する。
    pub(crate) fn safe_join(&self, parts: &[&str]) -> Result<PathBuf, AppError> {
        for part in parts {
            if part.contains('\x00') {
                return Err(AppError::path_security("パスに NUL バイトが含まれています"));
            }
            if Path::new(part).is_absolute() {
                return Err(AppError::path_security("絶対パスは指定できません"));
            }
        }

        let mut joined = self.roots[0].clone();
        for part in parts {
            joined = joined.join(part);
        }
        self.validate(&joined)
    }

    /// `validate` 済みディレクトリの直接の子を検証する (軽量版)
    ///
    /// 親が検証済みなので、子自身の symlink チェックのみ行う。
    pub(crate) fn validate_child(
        &self,
        child: &Path,
        is_symlink: bool,
    ) -> Result<PathBuf, AppError> {
        if !self.is_allow_symlinks && is_symlink {
            return Err(AppError::path_security(
                "symlink の追跡は許可されていません",
            ));
        }

        let resolved = if is_symlink {
            resolve_path(child)?
        } else {
            child.to_path_buf()
        };

        if !self.is_under_root(&resolved) {
            return Err(AppError::path_security(
                "許可ルートディレクトリの外へのアクセスは禁止されています",
            ));
        }
        Ok(resolved)
    }

    /// マウントポイントの slug が安全であることを検証する
    ///
    /// slug は `MOUNT_BASE_DIR` 直下のディレクトリ名。
    /// パストラバーサルに利用できる文字列を拒否する。
    pub(crate) fn validate_slug(slug: &str) -> Result<(), AppError> {
        if slug.is_empty() || slug == "." {
            return Err(AppError::path_security("slug が空または '.' です"));
        }
        if slug.contains('\x00') {
            return Err(AppError::path_security(
                "slug に NUL バイトが含まれています",
            ));
        }
        if slug.contains('/') || slug.contains('\\') {
            return Err(AppError::path_security(
                "slug にパス区切り文字が含まれています",
            ));
        }
        if slug == ".." || slug.starts_with("..") {
            return Err(AppError::path_security(
                "slug に不正なパス要素が含まれています",
            ));
        }
        Ok(())
    }

    /// `resolved` パスが許可ルートのいずれか配下にあるか判定する
    fn is_under_root(&self, resolved: &Path) -> bool {
        let s = resolved.to_string_lossy();
        for (root_str, root_prefix, _) in &self.root_entries {
            if *s == *root_str || s.starts_with(root_prefix.as_str()) {
                return true;
            }
        }
        false
    }

    /// パスのいずれかの要素が symlink かどうかを検出する
    ///
    /// 元のパス (resolve 前) の各要素を該当ルートから順に確認する。
    fn has_symlink(&self, path: &Path) -> bool {
        // 絶対パスに変換
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(path),
                Err(_) => return true,
            }
        };

        // パスが属するルートを特定 (resolve 済みで find)
        let Ok(resolved) = std::fs::canonicalize(&abs_path) else {
            // canonicalize 失敗 = パス自体が存在しない
            // 存在しないパスに symlink はないが、親パスに symlink がある可能性
            // 親を canonicalize して確認
            return Self::has_symlink_in_ancestors(&abs_path);
        };
        let Some(root) = self.find_root_for(&resolved) else {
            return true;
        };

        // raw パスからルートを除去して相対パスを取得
        // ルートは canonicalize 済みなので、raw パスと一致しない場合がある
        // 全ルートの raw パス/canonical パスで strip_prefix を試行
        let rel = if let Ok(r) = abs_path.strip_prefix(root) {
            r.to_path_buf()
        } else {
            // root_dirs のいずれかの元パスで strip_prefix を試行
            // (canonical root と raw path の不一致を解消)
            return false; // ルートから相対パスが取れない場合は symlink なしと判定
        };

        // 各要素を順にチェック
        let mut current = root.to_path_buf();
        for part in rel.components() {
            current = current.join(part);
            if current
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return true;
            }
        }
        false
    }

    /// 祖先パスに symlink があるか確認する (パス自体が存在しない場合用)
    fn has_symlink_in_ancestors(abs_path: &Path) -> bool {
        let mut current = PathBuf::new();
        for component in abs_path.components() {
            current = current.join(component);
            if !current.exists() {
                // これ以降は存在しないので symlink もない
                return false;
            }
            if current
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return true;
            }
        }
        false
    }
}

/// パスを正規化する
///
/// パスを正規化する
///
/// `canonicalize()` を試み、失敗時 (パスが存在しない等) は
/// 親ディレクトリを `canonicalize` してファイル名を append する。
/// 親も存在しない場合は手動で `..` を正規化する。
fn resolve_path(path: &Path) -> Result<PathBuf, AppError> {
    // 存在するパスは canonicalize で正確に解決
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Ok(canonical);
    }

    // 親ディレクトリが存在すれば canonicalize + ファイル名 append
    // (symlink 解決済みのルートと一致するようにするため)
    if let Some(parent) = path.parent() {
        if let Ok(canonical_parent) = std::fs::canonicalize(parent) {
            if let Some(file_name) = path.file_name() {
                return Ok(canonical_parent.join(file_name));
            }
        }
    }

    // 親も存在しない場合は手動で正規化 (traversal 検出用)
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| AppError::path_security("カレントディレクトリの取得に失敗"))?
            .join(path)
    };

    // `.` と `..` を正規化
    let mut components = Vec::new();
    for component in abs.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }

    let normalized: PathBuf = components.iter().collect();
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use tempfile::TempDir;

    // --- テストヘルパー ---

    struct TestEnv {
        #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
        dir: TempDir,
        root: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = dir.path().to_path_buf();
            fs::write(root.join("file.txt"), "hello").unwrap();
            fs::create_dir_all(root.join("subdir")).unwrap();
            fs::write(root.join("subdir/nested.txt"), "nested").unwrap();
            Self { dir, root }
        }

        fn security(&self) -> PathSecurity {
            PathSecurity::new(vec![self.root.clone()], false).unwrap()
        }
    }

    struct MultiTestEnv {
        dir: TempDir,
        root_a: PathBuf,
        root_b: PathBuf,
    }

    impl MultiTestEnv {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root_a = dir.path().join("root_a");
            let root_b = dir.path().join("root_b");
            fs::create_dir_all(&root_a).unwrap();
            fs::create_dir_all(&root_b).unwrap();
            fs::write(root_a.join("file_a.txt"), "a").unwrap();
            fs::write(root_b.join("file_b.txt"), "b").unwrap();
            fs::create_dir_all(root_a.join("shared_name")).unwrap();
            fs::create_dir_all(root_b.join("shared_name")).unwrap();
            Self {
                dir,
                root_a,
                root_b,
            }
        }

        fn security(&self) -> PathSecurity {
            PathSecurity::new(vec![self.root_a.clone(), self.root_b.clone()], false).unwrap()
        }
    }

    // --- 基本 validate テスト ---

    #[test]
    fn root_dir直下のファイルを許可する() {
        let env = TestEnv::new();
        let sec = env.security();
        let result = sec.validate(&env.root.join("file.txt")).unwrap();
        assert_eq!(result, fs::canonicalize(env.root.join("file.txt")).unwrap());
    }

    #[test]
    fn root_dir直下のサブディレクトリを許可する() {
        let env = TestEnv::new();
        let sec = env.security();
        let result = sec.validate(&env.root.join("subdir/nested.txt")).unwrap();
        assert_eq!(
            result,
            fs::canonicalize(env.root.join("subdir/nested.txt")).unwrap()
        );
    }

    #[test]
    fn root_dir自体を許可する() {
        let env = TestEnv::new();
        let sec = env.security();
        let result = sec.validate(&env.root).unwrap();
        assert_eq!(result, fs::canonicalize(&env.root).unwrap());
    }

    #[test]
    fn ドットドットによるtraversalを拒否する() {
        let env = TestEnv::new();
        let sec = env.security();
        let err = sec
            .validate(&env.root.join("../../etc/passwd"))
            .unwrap_err();
        assert!(err.to_string().contains("禁止"));
    }

    #[test]
    fn resolve後にroot_dir外になるパスを拒否する() {
        let env = TestEnv::new();
        let sec = env.security();
        let err = sec
            .validate(&env.root.join("subdir/../../etc"))
            .unwrap_err();
        assert!(err.to_string().contains("禁止"));
    }

    // --- safe_join ---

    #[test]
    fn 絶対パスのsafe_joinを拒否する() {
        let env = TestEnv::new();
        let sec = env.security();
        let err = sec.safe_join(&["/etc/passwd"]).unwrap_err();
        assert!(err.to_string().contains("絶対パス"));
    }

    #[test]
    fn nulバイトを含むパスを拒否する() {
        let env = TestEnv::new();
        let sec = env.security();
        let err = sec.safe_join(&["file\x00.txt"]).unwrap_err();
        assert!(err.to_string().contains("NUL"));
    }

    #[test]
    fn safe_joinで正常なパスを結合する() {
        let env = TestEnv::new();
        let sec = env.security();
        let result = sec.safe_join(&["subdir", "nested.txt"]).unwrap();
        assert_eq!(
            result,
            fs::canonicalize(env.root.join("subdir/nested.txt")).unwrap()
        );
    }

    // --- symlink ---

    #[test]
    fn symlinkがデフォルトで拒否される() {
        let env = TestEnv::new();
        let sec = env.security();
        let link = env.root.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(env.root.join("subdir"), &link).unwrap();
        let err = sec.validate(&link.join("nested.txt")).unwrap_err();
        assert!(err.to_string().contains("symlink"));
    }

    #[test]
    fn allow_symlinks有効時にsymlinkを許可する() {
        let env = TestEnv::new();
        let sec = PathSecurity::new(vec![env.root.clone()], true).unwrap();
        let link = env.root.join("link_allow");
        #[cfg(unix)]
        std::os::unix::fs::symlink(env.root.join("subdir"), &link).unwrap();
        let result = sec.validate(&link.join("nested.txt")).unwrap();
        assert_eq!(
            result,
            fs::canonicalize(env.root.join("subdir/nested.txt")).unwrap()
        );
    }

    // --- validate_existing ---

    #[test]
    fn validate_existingで存在しないパスがエラー() {
        let env = TestEnv::new();
        let sec = env.security();
        let err = sec
            .validate_existing(&env.root.join("nonexistent.txt"))
            .unwrap_err();
        assert!(err.to_string().contains("存在しません"));
    }

    // --- 複数ルート対応テスト ---

    #[test]
    fn 単一ルートでroot_dirsが1要素のリストを返す() {
        let env = TestEnv::new();
        let sec = env.security();
        assert_eq!(sec.root_dirs().len(), 1);
        assert_eq!(sec.root_dirs()[0], fs::canonicalize(&env.root).unwrap());
    }

    #[test]
    fn 複数ルートでroot_dirsが全ルートを返す() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        assert_eq!(sec.root_dirs().len(), 2);
    }

    #[test]
    fn root_a配下のファイルを許可する() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let result = sec.validate(&env.root_a.join("file_a.txt")).unwrap();
        assert_eq!(
            result,
            fs::canonicalize(env.root_a.join("file_a.txt")).unwrap()
        );
    }

    #[test]
    fn root_b配下のファイルを許可する() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let result = sec.validate(&env.root_b.join("file_b.txt")).unwrap();
        assert_eq!(
            result,
            fs::canonicalize(env.root_b.join("file_b.txt")).unwrap()
        );
    }

    #[test]
    fn どのルートにも属さないパスを拒否する() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let err = sec
            .validate(&env.dir.path().join("outside.txt"))
            .unwrap_err();
        assert!(err.to_string().contains("禁止"));
    }

    #[test]
    fn root_a配下からのトラバーサルを拒否する() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let err = sec
            .validate(&env.root_a.join("../../etc/passwd"))
            .unwrap_err();
        assert!(err.to_string().contains("禁止"));
    }

    // --- find_root_for ---

    #[test]
    fn root_a配下のパスに対してroot_aを返す() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let resolved = fs::canonicalize(env.root_a.join("file_a.txt")).unwrap();
        let root = sec.find_root_for(&resolved).unwrap();
        assert_eq!(root, fs::canonicalize(&env.root_a).unwrap());
    }

    #[test]
    fn root_b配下のパスに対してroot_bを返す() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let resolved = fs::canonicalize(env.root_b.join("file_b.txt")).unwrap();
        let root = sec.find_root_for(&resolved).unwrap();
        assert_eq!(root, fs::canonicalize(&env.root_b).unwrap());
    }

    #[test]
    fn どのルートにも属さないパスにnoneを返す() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let result = sec.find_root_for(Path::new("/tmp/outside.txt"));
        assert!(result.is_none());
    }

    #[test]
    fn ルート自体に対してルートを返す() {
        let env = MultiTestEnv::new();
        let sec = env.security();
        let root_a_resolved = fs::canonicalize(&env.root_a).unwrap();
        let result = sec.find_root_for(&root_a_resolved).unwrap();
        assert_eq!(result, root_a_resolved);
    }

    // --- validate_slug ---

    #[test]
    fn 正常なスラッグを許可する() {
        PathSecurity::validate_slug("photos").unwrap();
    }

    #[test]
    fn ハイフン付きスラッグを許可する() {
        PathSecurity::validate_slug("my-photos").unwrap();
    }

    #[test]
    fn 空のスラッグを拒否する() {
        assert!(PathSecurity::validate_slug("").is_err());
    }

    #[test]
    fn ドットのみのスラッグを拒否する() {
        assert!(PathSecurity::validate_slug(".").is_err());
    }

    #[test]
    fn ドットドットを拒否する() {
        assert!(PathSecurity::validate_slug("..").is_err());
    }

    #[test]
    fn nulバイトを含むスラッグを拒否する() {
        assert!(PathSecurity::validate_slug("test\x00slug").is_err());
    }

    #[test]
    fn スラッシュを含むスラッグを拒否する() {
        assert!(PathSecurity::validate_slug("path/traversal").is_err());
    }

    #[test]
    fn バックスラッシュを含むスラッグを拒否する() {
        assert!(PathSecurity::validate_slug("path\\traversal").is_err());
    }

    // --- validate_child ---

    #[test]
    fn validate_childで通常の子パスを許可する() {
        let env = TestEnv::new();
        let sec = env.security();
        let child = env.root.join("file.txt");
        let result = sec.validate_child(&child, false).unwrap();
        assert_eq!(result, child);
    }

    #[test]
    fn validate_childでsymlinkの子パスを拒否する() {
        let env = TestEnv::new();
        let sec = env.security();
        let link = env.root.join("link_child");
        #[cfg(unix)]
        std::os::unix::fs::symlink(env.root.join("subdir"), &link).unwrap();
        let err = sec.validate_child(&link, true).unwrap_err();
        assert!(err.to_string().contains("symlink"));
    }

    // --- 空ルートリスト ---

    #[test]
    fn 空のルートリストでエラー() {
        let err = PathSecurity::new(vec![], false).unwrap_err();
        assert!(err.to_string().contains("少なくとも1つ"));
    }
}
