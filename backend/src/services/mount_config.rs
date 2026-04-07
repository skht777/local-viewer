//! マウントポイント設定の読み込み
//!
//! `mounts.json` (v1/v2 スキーマ) を読み込み、マウントポイント定義を返す。
//! slug のバリデーションは `PathSecurity::validate_slug()` に委譲する。
//!
//! スキーマ:
//!   v1: `mount_id`, name, path (コンテナ内絶対パス)
//!   v2: `mount_id`, name, slug (`MOUNT_BASE_DIR` からの相対名), `host_path`

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::errors::AppError;
use crate::services::path_security::PathSecurity;

/// マウントポイント定義
pub(crate) struct MountPoint {
    pub mount_id: String,
    pub name: String,
    pub slug: String,
    pub host_path: String,
}

impl MountPoint {
    /// slug からコンテナ内の絶対パスを導出する
    ///
    /// - "." は `base_dir` 自体を返す (`ROOT_DIR` マイグレーション互換)
    /// - それ以外は `validate_slug()` で安全性を検証した上で `base_dir` / slug を resolve
    /// - resolve 後が `base_dir` 配下であることを防御的に確認
    pub(crate) fn resolve_path(&self, base_dir: &Path) -> Result<PathBuf, AppError> {
        let base_resolved = std::fs::canonicalize(base_dir).map_err(|_| {
            AppError::path_security(format!(
                "MOUNT_BASE_DIR の解決に失敗: {}",
                base_dir.display()
            ))
        })?;

        if self.slug == "." {
            return Ok(base_resolved);
        }

        PathSecurity::validate_slug(&self.slug)?;
        let candidate = base_dir.join(&self.slug);
        let resolved = std::fs::canonicalize(&candidate).map_err(|_| {
            AppError::path_security(format!(
                "マウントポイントパスの解決に失敗: {}",
                candidate.display()
            ))
        })?;

        // base_dir 配下であることを防御的に確認
        let base_str = base_resolved.to_string_lossy();
        let resolved_str = resolved.to_string_lossy();
        if resolved_str.as_ref() != base_str.as_ref()
            && !resolved_str
                .as_ref()
                .starts_with(&format!("{base_str}{}", std::path::MAIN_SEPARATOR))
        {
            return Err(AppError::path_security(
                "slug が MOUNT_BASE_DIR 外を参照しています",
            ));
        }

        Ok(resolved)
    }
}

/// マウントポイント設定全体
pub(crate) struct MountConfig {
    pub mounts: Vec<MountPoint>,
}

// --- JSON デシリアライズ用の中間構造体 ---

#[derive(Deserialize)]
struct RawMountConfig {
    #[serde(default)]
    mounts: Vec<RawMountEntry>,
}

#[derive(Deserialize)]
struct RawMountEntry {
    mount_id: String,
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    host_path: String,
    // v1 互換: path フィールド
    path: Option<String>,
}

/// mounts.json を読み込む
///
/// - ファイル不在 → 空の `MountConfig` を返す
/// - v1 互換: "path" フィールドから slug を導出
pub(crate) fn load_mount_config(
    config_path: &Path,
    base_dir: &Path,
) -> Result<MountConfig, AppError> {
    if !config_path.exists() {
        return Ok(MountConfig { mounts: vec![] });
    }

    let content = std::fs::read_to_string(config_path)
        .map_err(|e| AppError::path_security(format!("設定ファイルの読み込みに失敗: {e}")))?;

    let raw: RawMountConfig = serde_json::from_str(&content)
        .map_err(|e| AppError::path_security(format!("設定ファイルのパースに失敗: {e}")))?;

    let base_resolved = std::fs::canonicalize(base_dir).unwrap_or_else(|_| base_dir.to_path_buf());

    let mounts = raw
        .mounts
        .into_iter()
        .map(|m| {
            let slug = if m.slug.is_empty() {
                // v1 互換: path から slug を導出
                m.path
                    .as_deref()
                    .map(|p| derive_slug_from_path(p, &base_resolved))
                    .unwrap_or_default()
            } else {
                m.slug
            };
            MountPoint {
                mount_id: m.mount_id,
                name: m.name,
                slug,
                host_path: m.host_path,
            }
        })
        .collect();

    Ok(MountConfig { mounts })
}

/// v1 の path フィールドから slug を導出する
///
/// - `base_dir` と一致 → "."
/// - `base_dir` 配下 → 相対パス
/// - それ以外 → basename
fn derive_slug_from_path(path_str: &str, base_dir: &Path) -> String {
    let Ok(resolved) = std::fs::canonicalize(path_str) else {
        return Path::new(path_str)
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    };

    if resolved == base_dir {
        return ".".to_string();
    }

    resolved.strip_prefix(base_dir).ok().map_or_else(
        || {
            resolved
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        },
        |rel| rel.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use tempfile::TempDir;

    struct TestEnv {
        #[allow(dead_code, reason = "TempDir のドロップでディレクトリを保持")]
        dir: TempDir,
        base_dir: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let base_dir = fs::canonicalize(dir.path()).unwrap();
            // サブディレクトリを作成
            fs::create_dir_all(base_dir.join("pictures")).unwrap();
            fs::create_dir_all(base_dir.join("videos")).unwrap();
            Self { dir, base_dir }
        }

        fn write_mounts_json(&self, content: &str) -> PathBuf {
            let path = self.base_dir.join("mounts.json");
            fs::write(&path, content).unwrap();
            path
        }
    }

    #[test]
    fn v2形式のmounts_jsonを正しく読み込む() {
        let env = TestEnv::new();
        let path = env.write_mounts_json(
            r#"{
              "version": 2,
              "mounts": [
                {"mount_id": "abc123", "name": "Pictures", "slug": "pictures", "host_path": "/home/user/pics"}
              ]
            }"#,
        );
        let config = load_mount_config(&path, &env.base_dir).unwrap();
        assert_eq!(config.mounts.len(), 1);
        assert_eq!(config.mounts[0].mount_id, "abc123");
        assert_eq!(config.mounts[0].name, "Pictures");
        assert_eq!(config.mounts[0].slug, "pictures");
        assert_eq!(config.mounts[0].host_path, "/home/user/pics");
    }

    #[test]
    fn v1形式からslugを導出する() {
        let env = TestEnv::new();
        let pictures_path = env.base_dir.join("pictures");
        let json = format!(
            "{{\n\"version\": 1,\n\"mounts\": [\n{{\"mount_id\": \"abc123\", \"name\": \"Pics\", \"path\": \"{}\"}}\n]\n}}",
            pictures_path.display()
        );
        let path = env.write_mounts_json(&json);
        let config = load_mount_config(&path, &env.base_dir).unwrap();
        assert_eq!(config.mounts[0].slug, "pictures");
    }

    #[test]
    fn v1でbase_dir自体はslugドットになる() {
        let env = TestEnv::new();
        let json = format!(
            "{{\n\"version\": 1,\n\"mounts\": [\n{{\"mount_id\": \"abc123\", \"name\": \"Root\", \"path\": \"{}\"}}\n]\n}}",
            env.base_dir.display()
        );
        let path = env.write_mounts_json(&json);
        let config = load_mount_config(&path, &env.base_dir).unwrap();
        assert_eq!(config.mounts[0].slug, ".");
    }

    #[test]
    fn ファイル不在で空のconfigを返す() {
        let env = TestEnv::new();
        let nonexistent = env.base_dir.join("nonexistent.json");
        let config = load_mount_config(&nonexistent, &env.base_dir).unwrap();
        assert!(config.mounts.is_empty());
    }

    #[test]
    fn resolve_pathでslugドットがbase_dirを返す() {
        let env = TestEnv::new();
        let mp = MountPoint {
            mount_id: "abc".to_string(),
            name: "Root".to_string(),
            slug: ".".to_string(),
            host_path: String::new(),
        };
        let resolved = mp.resolve_path(&env.base_dir).unwrap();
        assert_eq!(resolved, env.base_dir);
    }

    #[test]
    fn resolve_pathで通常slugがbase_dir配下を返す() {
        let env = TestEnv::new();
        let mp = MountPoint {
            mount_id: "abc".to_string(),
            name: "Pictures".to_string(),
            slug: "pictures".to_string(),
            host_path: String::new(),
        };
        let resolved = mp.resolve_path(&env.base_dir).unwrap();
        assert_eq!(resolved, env.base_dir.join("pictures"));
    }

    #[test]
    fn 不正slugでpathsecurityエラーを返す() {
        let env = TestEnv::new();
        let mp = MountPoint {
            mount_id: "abc".to_string(),
            name: "Bad".to_string(),
            slug: "../escape".to_string(),
            host_path: String::new(),
        };
        let result = mp.resolve_path(&env.base_dir);
        assert!(result.is_err());
    }

    #[test]
    fn 不正なjsonでエラーを返す() {
        let env = TestEnv::new();
        let path = env.write_mounts_json("not valid json");
        let result = load_mount_config(&path, &env.base_dir);
        assert!(result.is_err());
    }

    #[test]
    fn 複数マウントポイントを読み込む() {
        let env = TestEnv::new();
        let path = env.write_mounts_json(
            r#"{
              "version": 2,
              "mounts": [
                {"mount_id": "m1", "name": "Pictures", "slug": "pictures"},
                {"mount_id": "m2", "name": "Videos", "slug": "videos"}
              ]
            }"#,
        );
        let config = load_mount_config(&path, &env.base_dir).unwrap();
        assert_eq!(config.mounts.len(), 2);
        assert_eq!(config.mounts[0].slug, "pictures");
        assert_eq!(config.mounts[1].slug, "videos");
    }
}
