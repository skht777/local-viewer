//! アーカイブサービス
//!
//! ZIP/RAR/7z の統一インターフェース + moka キャッシュ。
//! リーダー選択、エントリ一覧キャッシュ、抽出バイトキャッシュを管理する。

pub(crate) mod rar_reader;
pub(crate) mod reader;
pub(crate) mod security;
pub(crate) mod sevenz_reader;
pub(crate) mod zip_reader;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;

use self::rar_reader::RarArchiveReader;
use self::reader::{ArchiveEntry, ArchiveReader};
use self::security::{ArchiveEntryValidator, is_video_extension};
use self::sevenz_reader::SevenZipArchiveReader;
use self::zip_reader::ZipArchiveReader;
use crate::config::Settings;
use crate::errors::AppError;

/// アーカイブサービス
///
/// - リーダー選択 (ZIP → RAR → 7z)
/// - `list_cache`: エントリ一覧を mtime ベースでキャッシュ (max 32)
/// - `entry_cache`: 抽出バイトを moka W-TinyLFU でキャッシュ (バイト重み付き)
/// - 動画エントリは `entry_cache` をバイパス (OOM 防止)
pub(crate) struct ArchiveService {
    readers: Vec<Box<dyn ArchiveReader>>,
    list_cache: moka::sync::Cache<String, Arc<Vec<ArchiveEntry>>>,
    entry_cache: moka::sync::Cache<String, Bytes>,
}

impl ArchiveService {
    /// `ArchiveService` を構築する
    ///
    /// 各リーダーの利用可否を起動時にチェックし、diagnostics としてログ出力可能にする。
    pub(crate) fn new(settings: &Settings) -> Self {
        // 各リーダー用にバリデータを作成 (Clone 不可のため個別生成)
        let zip_validator = ArchiveEntryValidator::new(settings);
        let rar_validator = ArchiveEntryValidator::new(settings);
        let sevenz_validator = ArchiveEntryValidator::new(settings);

        let readers: Vec<Box<dyn ArchiveReader>> = vec![
            Box::new(ZipArchiveReader::new(zip_validator)),
            Box::new(RarArchiveReader::new(rar_validator)),
            Box::new(SevenZipArchiveReader::new(sevenz_validator)),
        ];

        // list_cache: エントリ一覧メタデータ (max 32)
        let list_cache = moka::sync::Cache::builder().max_capacity(32).build();

        // entry_cache: 抽出バイトデータ (バイト重み付き制限)
        let max_bytes = u64::from(settings.archive_cache_mb) * 1024 * 1024;
        let entry_cache = moka::sync::Cache::builder()
            .weigher(|_key: &String, value: &Bytes| -> u32 {
                value.len().try_into().unwrap_or(u32::MAX)
            })
            .max_capacity(max_bytes)
            .build();

        Self {
            readers,
            list_cache,
            entry_cache,
        }
    }

    /// テスト用: カスタムリーダーで構築する
    #[cfg(test)]
    fn with_readers(readers: Vec<Box<dyn ArchiveReader>>) -> Self {
        let list_cache = moka::sync::Cache::builder().max_capacity(32).build();
        let entry_cache = moka::sync::Cache::builder()
            .weigher(|_key: &String, value: &Bytes| -> u32 {
                value.len().try_into().unwrap_or(u32::MAX)
            })
            .max_capacity(256 * 1024 * 1024)
            .build();
        Self {
            readers,
            list_cache,
            entry_cache,
        }
    }

    /// パスに対応するリーダーを返す
    fn get_reader(&self, path: &Path) -> Option<&dyn ArchiveReader> {
        self.readers
            .iter()
            .find(|r| r.supports(path))
            .map(AsRef::as_ref)
    }

    /// エントリ一覧を返す (mtime ベースキャッシュ)
    ///
    /// キャッシュキー: `"{path}:{mtime_ns}"`
    /// mtime が変わるとキャッシュミスし、新しい一覧を取得する。
    pub(crate) fn list_entries(
        &self,
        archive_path: &Path,
    ) -> Result<Arc<Vec<ArchiveEntry>>, AppError> {
        let reader = self.get_reader(archive_path).ok_or_else(|| {
            AppError::InvalidArchive(format!(
                "サポートされていないアーカイブ形式です: {}",
                archive_path.display()
            ))
        })?;

        let cache_key = make_list_cache_key(archive_path)?;

        if let Some(cached) = self.list_cache.get(&cache_key) {
            return Ok(cached);
        }

        let entries = reader.list_entries(archive_path)?;
        let arc_entries = Arc::new(entries);
        self.list_cache.insert(cache_key, Arc::clone(&arc_entries));
        Ok(arc_entries)
    }

    /// エントリを抽出する (キャッシュ付き)
    ///
    /// - 動画エントリはキャッシュをバイパスする (OOM 防止)
    /// - キャッシュキー: `"{path}:{mtime_ns}:{entry_name}"`
    pub(crate) fn extract_entry(
        &self,
        archive_path: &Path,
        entry_name: &str,
    ) -> Result<Bytes, AppError> {
        let reader = self.get_reader(archive_path).ok_or_else(|| {
            AppError::InvalidArchive(format!(
                "サポートされていないアーカイブ形式です: {}",
                archive_path.display()
            ))
        })?;

        // 動画エントリはキャッシュバイパス
        if is_video_extension(entry_name) {
            return reader.extract_entry(archive_path, entry_name);
        }

        let cache_key = make_entry_cache_key(archive_path, entry_name)?;

        if let Some(cached) = self.entry_cache.get(&cache_key) {
            return Ok(cached);
        }

        let data = reader.extract_entry(archive_path, entry_name)?;
        self.entry_cache.insert(cache_key, data.clone());
        Ok(data)
    }

    /// パスがサポート対象のアーカイブ形式か判定する
    pub(crate) fn is_supported(&self, path: &Path) -> bool {
        self.get_reader(path).is_some()
    }

    /// 先頭の画像エントリを返す
    ///
    /// アーカイブファイル自体のサムネイル生成用。
    /// 画像拡張子を持つ最初のエントリを返す (ディレクトリは除外)。
    pub(crate) fn first_image_entry(
        &self,
        archive_path: &Path,
    ) -> Result<Option<ArchiveEntry>, AppError> {
        let entries = self.list_entries(archive_path)?;
        Ok(entries
            .iter()
            .find(|e| !e.is_dir && is_image_entry(&e.name))
            .cloned())
    }

    /// 複数エントリを一括抽出する (バッチサムネイル用)
    ///
    /// 同一アーカイブの複数エントリを 1 回のアーカイブ読み込みで抽出する。
    /// キャッシュ済みエントリはスキップし、未キャッシュ分のみリーダーで抽出する。
    pub(crate) fn extract_entries_batch(
        &self,
        archive_path: &Path,
        entry_names: &[String],
    ) -> Result<HashMap<String, Bytes>, AppError> {
        let reader = self.get_reader(archive_path).ok_or_else(|| {
            AppError::InvalidArchive(format!(
                "サポートされていないアーカイブ形式です: {}",
                archive_path.display()
            ))
        })?;

        let mut results = HashMap::with_capacity(entry_names.len());
        let mut uncached = Vec::new();

        // キャッシュ済みエントリを収集
        for name in entry_names {
            if let Ok(cache_key) = make_entry_cache_key(archive_path, name) {
                if let Some(cached) = self.entry_cache.get(&cache_key) {
                    results.insert(name.clone(), cached);
                    continue;
                }
            }
            uncached.push(name.clone());
        }

        // 未キャッシュ分をリーダーで一括抽出
        if !uncached.is_empty() {
            let extracted = reader.extract_entries(archive_path, &uncached)?;
            for (name, data) in extracted {
                // 動画エントリ以外はキャッシュに登録
                if !is_video_extension(&name) {
                    if let Ok(cache_key) = make_entry_cache_key(archive_path, &name) {
                        self.entry_cache.insert(cache_key, data.clone());
                    }
                }
                results.insert(name, data);
            }
        }

        Ok(results)
    }

    /// 各アーカイブ形式の利用可否を返す
    pub(crate) fn get_diagnostics(&self) -> HashMap<String, bool> {
        let mut diag = HashMap::new();
        diag.insert("zip".to_string(), true); // zip クレートは常に利用可
        // RAR/7z はリーダーの is_available で判定
        for reader in &self.readers {
            if reader.supports(Path::new("dummy.rar")) {
                diag.insert("rar".to_string(), true);
            }
            if reader.supports(Path::new("dummy.7z")) {
                diag.insert("7z".to_string(), true);
            }
        }
        diag.entry("rar".to_string()).or_insert(false);
        diag.entry("7z".to_string()).or_insert(false);
        diag
    }
}

/// エントリ名が画像拡張子を持つかチェ��クする
fn is_image_entry(name: &str) -> bool {
    let lower = name.to_lowercase();
    crate::services::extensions::IMAGE_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// `list_cache` のキーを生成する: `"{path}:{mtime_ns}"`
fn make_list_cache_key(archive_path: &Path) -> Result<String, AppError> {
    let meta = std::fs::metadata(archive_path)
        .map_err(|e| AppError::InvalidArchive(format!("メタデータ取得失敗: {e}")))?;
    let mtime = meta
        .modified()
        .map_err(|e| AppError::InvalidArchive(format!("mtime 取得失敗: {e}")))?;
    let mtime_ns = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(format!("{}:{mtime_ns}", archive_path.display()))
}

/// `entry_cache` のキーを生成する: `"{path}:{mtime_ns}:{entry_name}"`
fn make_entry_cache_key(archive_path: &Path, entry_name: &str) -> Result<String, AppError> {
    let meta = std::fs::metadata(archive_path)
        .map_err(|e| AppError::InvalidArchive(format!("メタデータ取得失敗: {e}")))?;
    let mtime = meta
        .modified()
        .map_err(|e| AppError::InvalidArchive(format!("mtime 取得失敗: {e}")))?;
    let mtime_ns = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(format!(
        "{}:{mtime_ns}:{entry_name}",
        archive_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;

    use super::*;

    fn test_settings() -> Settings {
        Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            "/tmp".to_string(),
        )]))
        .unwrap()
    }

    fn create_test_zip(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let tmp = tempfile::NamedTempFile::with_suffix(".zip").unwrap();
        let mut writer = zip::ZipWriter::new(tmp.as_file().try_clone().unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        tmp
    }

    #[test]
    fn zipパスにリーダーが見つかる() {
        let svc = ArchiveService::new(&test_settings());
        assert!(svc.get_reader(Path::new("test.zip")).is_some());
    }

    #[test]
    fn サポート外の拡張子でnoneを返す() {
        let svc = ArchiveService::new(&test_settings());
        assert!(svc.get_reader(Path::new("test.txt")).is_none());
    }

    #[test]
    fn list_entriesがエントリ一覧を返す() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("a.jpg", b"data_a"), ("b.png", b"data_b")]);

        let entries = svc.list_entries(zip.path()).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn list_entriesがキャッシュヒットする() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("a.jpg", b"data")]);

        let entries1 = svc.list_entries(zip.path()).unwrap();
        let entries2 = svc.list_entries(zip.path()).unwrap();
        // Arc のポインタが同一 (キャッシュヒット)
        assert!(Arc::ptr_eq(&entries1, &entries2));
    }

    #[test]
    fn extract_entryがバイトデータを返す() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("photo.jpg", b"hello")]);

        let data = svc.extract_entry(zip.path(), "photo.jpg").unwrap();
        assert_eq!(&data[..], b"hello");
    }

    #[test]
    fn extract_entryがキャッシュヒットする() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("photo.jpg", b"hello")]);

        let data1 = svc.extract_entry(zip.path(), "photo.jpg").unwrap();
        let data2 = svc.extract_entry(zip.path(), "photo.jpg").unwrap();
        assert_eq!(data1, data2);
    }

    #[test]
    fn 動画エントリがキャッシュをバイパスする() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("video.mp4", b"video data")]);

        // 動画でもデータは取得可能
        let data = svc.extract_entry(zip.path(), "video.mp4").unwrap();
        assert_eq!(&data[..], b"video data");

        // entry_cache には入っていない
        let cache_key = make_entry_cache_key(zip.path(), "video.mp4").unwrap();
        assert!(svc.entry_cache.get(&cache_key).is_none());
    }

    #[test]
    fn get_diagnosticsがzip_trueを含む() {
        let svc = ArchiveService::new(&test_settings());
        let diag = svc.get_diagnostics();
        assert!(diag["zip"]);
    }

    #[test]
    fn is_supportedがzipでtrueを返す() {
        let svc = ArchiveService::new(&test_settings());
        assert!(svc.is_supported(Path::new("test.zip")));
    }

    #[test]
    fn is_supportedがtxtでfalseを返す() {
        let svc = ArchiveService::new(&test_settings());
        assert!(!svc.is_supported(Path::new("test.txt")));
    }

    #[test]
    fn first_image_entryが先頭画像を返す() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("a.jpg", b"img_a"), ("b.png", b"img_b")]);

        let entry = svc.first_image_entry(zip.path()).unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name, "a.jpg");
    }

    #[test]
    fn first_image_entryが画像なしでnoneを返す() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("readme.txt", b"text")]);

        // txt は拡張子フィルタで list_entries から除外されるため空リスト
        let entry = svc.first_image_entry(zip.path()).unwrap();
        assert!(entry.is_none());
    }

    #[test]
    fn extract_entries_batchが複数エントリを一括抽出する() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("a.jpg", b"data_a"), ("b.png", b"data_b")]);

        let names = vec!["a.jpg".to_string(), "b.png".to_string()];
        let results = svc.extract_entries_batch(zip.path(), &names).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(&results["a.jpg"][..], b"data_a");
        assert_eq!(&results["b.png"][..], b"data_b");
    }

    #[test]
    fn extract_entries_batchがキャッシュ済みエントリを再利用する() {
        let svc = ArchiveService::new(&test_settings());
        let zip = create_test_zip(&[("a.jpg", b"data_a")]);

        // 事前にキャッシュにエントリを載せる
        let _ = svc.extract_entry(zip.path(), "a.jpg").unwrap();

        let names = vec!["a.jpg".to_string()];
        let results = svc.extract_entries_batch(zip.path(), &names).unwrap();
        assert_eq!(&results["a.jpg"][..], b"data_a");
    }
}
