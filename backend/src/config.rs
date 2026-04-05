//! 環境変数ベースの設定モジュール
//!
//! Python 版 `config.py` と同一の変数名・デフォルト値を使用する。

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

/// 設定エラー
#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("{0}")]
    Missing(String),
    #[error("{key}: 不正な値 '{value}' — {reason}")]
    Invalid {
        key: String,
        value: String,
        reason: String,
    },
    #[error("{key}: パスの解決に失敗 — {source}")]
    PathResolve {
        key: String,
        #[source]
        source: std::io::Error,
    },
}

/// アプリケーション設定
///
/// 全フィールドは Python 版 `Settings` と同一のデフォルト値を持つ。
#[derive(Debug)]
#[allow(dead_code, reason = "Phase 3+ で追加のフィールドを使用")]
pub(crate) struct Settings {
    // マウントポイント設定
    pub mount_base_dir: PathBuf,
    pub mount_config_path: String,
    pub is_allow_symlinks: bool,

    // アーカイブ安全性設定
    pub archive_max_total_size: u64,
    pub archive_max_entry_size: u64,
    pub archive_max_video_entry_size: u64,
    pub archive_max_ratio: f64,
    pub archive_cache_mb: u32,
    pub archive_disk_cache_mb: u32,
    pub archive_registry_max_entries: usize,

    // 動画変換設定
    pub video_remux_timeout: u64,
    pub video_thumb_seek_seconds: u64,
    pub video_thumb_timeout: u64,

    // 検索/インデックス設定
    pub index_db_path: String,
    pub watch_mode: String,
    pub watch_poll_interval: u64,
    pub rebuild_rate_limit_seconds: u64,
    pub search_max_results: usize,
    pub search_query_timeout: u64,

    // 並列処理
    pub scan_workers: usize,
}

impl Settings {
    /// 環境変数から設定を読み込む
    ///
    /// `MOUNT_BASE_DIR` が未設定または空の場合はエラーを返す。
    /// 不正値は panic ではなくエラーメッセージ付きで返す。
    pub(crate) fn new() -> Result<Self, ConfigError> {
        let env_map: HashMap<String, String> = env::vars().collect();
        Self::from_map(&env_map)
    }

    /// キーバリューマップから設定を読み込む (テスト用)
    pub(crate) fn from_map(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let mount_base = vars.get("MOUNT_BASE_DIR").map_or("", String::as_str);
        if mount_base.is_empty() {
            return Err(ConfigError::Missing(
                "MOUNT_BASE_DIR を設定してください".to_string(),
            ));
        }
        let mount_base_dir =
            std::fs::canonicalize(mount_base).map_err(|e| ConfigError::PathResolve {
                key: "MOUNT_BASE_DIR".to_string(),
                source: e,
            })?;

        let is_allow_symlinks = matches!(
            vars.get("ALLOW_SYMLINKS")
                .map_or("", String::as_str)
                .to_lowercase()
                .as_str(),
            "true" | "1" | "yes"
        );

        let get = |key: &str| vars.get(key).map(String::as_str);

        Ok(Self {
            mount_base_dir,
            mount_config_path: get("MOUNT_CONFIG_PATH")
                .unwrap_or("config/mounts.json")
                .to_string(),
            is_allow_symlinks,

            archive_max_total_size: parse_or(get("ARCHIVE_MAX_TOTAL_SIZE"), 1024 * 1024 * 1024)?,
            archive_max_entry_size: parse_or(get("ARCHIVE_MAX_ENTRY_SIZE"), 32 * 1024 * 1024)?,
            archive_max_video_entry_size: parse_or(
                get("ARCHIVE_MAX_VIDEO_ENTRY_SIZE"),
                500 * 1024 * 1024,
            )?,
            archive_max_ratio: parse_or(get("ARCHIVE_MAX_RATIO"), 100.0)?,
            archive_cache_mb: parse_or(get("ARCHIVE_CACHE_MB"), 256)?,
            archive_disk_cache_mb: parse_or(get("ARCHIVE_DISK_CACHE_MB"), 1024)?,
            archive_registry_max_entries: parse_or(get("ARCHIVE_REGISTRY_MAX_ENTRIES"), 100_000)?,

            video_remux_timeout: parse_or(get("VIDEO_REMUX_TIMEOUT"), 120)?,
            video_thumb_seek_seconds: parse_or(get("VIDEO_THUMB_SEEK_SECONDS"), 1)?,
            video_thumb_timeout: parse_or(get("VIDEO_THUMB_TIMEOUT"), 10)?,

            index_db_path: get("INDEX_DB_PATH")
                .unwrap_or("/tmp/viewer-index.db")
                .to_string(),
            watch_mode: get("WATCH_MODE").unwrap_or("auto").to_string(),
            watch_poll_interval: parse_or(get("WATCH_POLL_INTERVAL"), 30)?,
            rebuild_rate_limit_seconds: parse_or(get("REBUILD_RATE_LIMIT_SECONDS"), 60)?,
            search_max_results: parse_or(get("SEARCH_MAX_RESULTS"), 200)?,
            search_query_timeout: parse_or(get("SEARCH_QUERY_TIMEOUT"), 5)?,

            scan_workers: parse_or(get("SCAN_WORKERS"), 8)?,
        })
    }
}

/// 値文字列をパースして取得、未設定ならデフォルト値を返す
fn parse_or<T>(val: Option<&str>, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr + std::fmt::Display,
{
    match val {
        Some(s) => s.parse::<T>().map_err(|_| ConfigError::Invalid {
            key: String::new(),
            value: s.to_string(),
            reason: std::any::type_name::<T>().to_string(),
        }),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    fn base_vars() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("MOUNT_BASE_DIR".to_string(), "/tmp".to_string());
        m
    }

    #[test]
    fn mount_base_dirが未設定でエラー() {
        let vars = HashMap::new();
        let err = Settings::from_map(&vars).unwrap_err();
        assert!(err.to_string().contains("MOUNT_BASE_DIR"));
    }

    #[test]
    fn mount_base_dirが空文字列でエラー() {
        let mut vars = HashMap::new();
        vars.insert("MOUNT_BASE_DIR".to_string(), String::new());
        let err = Settings::from_map(&vars).unwrap_err();
        assert!(err.to_string().contains("MOUNT_BASE_DIR"));
    }

    #[test]
    fn mount_base_dirが設定されている場合にresolve済みパスを返す() {
        let settings = Settings::from_map(&base_vars()).unwrap();
        assert!(settings.mount_base_dir.is_absolute());
    }

    #[rstest]
    #[case("true")]
    #[case("1")]
    #[case("yes")]
    #[case("TRUE")]
    #[case("Yes")]
    fn allow_symlinksのtrue系でtrueになる(#[case] value: &str) {
        let mut vars = base_vars();
        vars.insert("ALLOW_SYMLINKS".to_string(), value.to_string());
        let settings = Settings::from_map(&vars).unwrap();
        assert!(settings.is_allow_symlinks);
    }

    #[test]
    fn allow_symlinksのデフォルトがfalse() {
        let settings = Settings::from_map(&base_vars()).unwrap();
        assert!(!settings.is_allow_symlinks);
    }

    #[test]
    fn アーカイブ設定のデフォルト値が正しい() {
        let settings = Settings::from_map(&base_vars()).unwrap();
        assert_eq!(settings.archive_max_total_size, 1024 * 1024 * 1024);
        assert_eq!(settings.archive_max_entry_size, 32 * 1024 * 1024);
        #[allow(clippy::float_cmp, reason = "デフォルト値の完全一致を検証")]
        {
            assert_eq!(settings.archive_max_ratio, 100.0);
        }
        assert_eq!(settings.archive_cache_mb, 256);
        assert_eq!(settings.archive_registry_max_entries, 100_000);
    }

    #[test]
    fn 環境変数でアーカイブ設定を上書きできる() {
        let mut vars = base_vars();
        vars.insert("ARCHIVE_CACHE_MB".to_string(), "512".to_string());
        let settings = Settings::from_map(&vars).unwrap();
        assert_eq!(settings.archive_cache_mb, 512);
    }

    #[test]
    fn 不正な数値でconfigerror() {
        let mut vars = base_vars();
        vars.insert("ARCHIVE_CACHE_MB".to_string(), "not_a_number".to_string());
        let err = Settings::from_map(&vars).unwrap_err();
        assert!(err.to_string().contains("not_a_number"));
    }
}
