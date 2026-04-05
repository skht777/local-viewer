//! ZIP/CBZ アーカイブリーダー
//!
//! `zip` クレートで ZIP/CBZ を読み取り、
//! セキュリティ検証 + 拡張子フィルタ + 自然順ソート済みのエントリ一覧を返す。
//! 抽出時はチャンク読み (64KiB) でサイズ上限を超えたら中断する。

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use bytes::Bytes;

use super::reader::{ArchiveEntry, ArchiveReader};
use super::security::ArchiveEntryValidator;
use crate::errors::AppError;
use crate::services::natural_sort::natural_sort_key;

/// チャンク読みサイズ (64KiB)
const EXTRACT_CHUNK_SIZE: usize = 64 * 1024;

/// ZIP 拡張子
const ZIP_EXTENSIONS: &[&str] = &[".zip", ".cbz"];

/// ZIP/CBZ アーカイブリーダー
pub(crate) struct ZipArchiveReader {
    validator: ArchiveEntryValidator,
}

impl ZipArchiveReader {
    pub(crate) fn new(validator: ArchiveEntryValidator) -> Self {
        Self { validator }
    }

    /// ZIP エントリをチャンク読みで抽出する (サイズ上限付き)
    fn extract_from_zip(
        &self,
        archive: &mut zip::ZipArchive<std::fs::File>,
        entry_name: &str,
    ) -> Result<Bytes, AppError> {
        let max_size = self.validator.max_entry_size_for(entry_name);

        let mut file = archive.by_name(entry_name).map_err(|e| match e {
            zip::result::ZipError::FileNotFound => {
                AppError::InvalidArchive(format!("エントリが見つかりません: {entry_name}"))
            }
            _ => AppError::InvalidArchive(format!("ZIP エントリ読み取りエラー: {e}")),
        })?;

        let mut buf = Vec::with_capacity(file.size() as usize);
        let mut chunk = vec![0u8; EXTRACT_CHUNK_SIZE];
        let mut total: u64 = 0;

        loop {
            let n = file
                .read(&mut chunk)
                .map_err(|e| AppError::InvalidArchive(format!("ZIP 読み取りエラー: {e}")))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > max_size {
                return Err(AppError::ArchiveSecurity(format!(
                    "抽出時にサイズ上限を超えました: {entry_name}"
                )));
            }
            buf.extend_from_slice(&chunk[..n]);
        }

        Ok(Bytes::from(buf))
    }
}

impl ArchiveReader for ZipArchiveReader {
    fn list_entries(&self, archive_path: &Path) -> Result<Vec<ArchiveEntry>, AppError> {
        let file = std::fs::File::open(archive_path)
            .map_err(|e| AppError::InvalidArchive(format!("ファイルを開けません: {e}")))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP を読み取れません: {e}")))?;

        let mut entries = Vec::new();
        let mut total_uncompressed: u64 = 0;

        for i in 0..archive.len() {
            let info = archive.by_index_raw(i).map_err(|e| {
                AppError::InvalidArchive(format!("ZIP エントリ読み取りエラー: {e}"))
            })?;

            // パスワード付き検出 (encrypted flag)
            if info.encrypted() {
                return Err(AppError::ArchivePassword(
                    "パスワード付きアーカイブは未対応です".to_string(),
                ));
            }

            // ディレクトリエントリ除外
            if info.is_dir() {
                continue;
            }

            // バックスラッシュを正規化
            let name = info.name().replace('\\', "/");

            // エントリ名セキュリティ検証 (不正エントリは個別スキップ)
            if ArchiveEntryValidator::validate_entry_name(&name).is_err() {
                continue;
            }

            // 許可拡張子チェック
            if !ArchiveEntryValidator::is_allowed_extension(&name) {
                continue;
            }

            // サイズ検証 (超過エントリは個別スキップ)
            if self
                .validator
                .validate_entry_size(info.compressed_size(), info.size(), &name)
                .is_err()
            {
                continue;
            }
            total_uncompressed += info.size();

            entries.push(ArchiveEntry {
                name,
                size_compressed: info.compressed_size(),
                size_uncompressed: info.size(),
                is_dir: false,
            });
        }

        // 合計サイズ検証
        self.validator.validate_total_size(total_uncompressed)?;

        // 自然順ソート
        entries.sort_by(|a, b| natural_sort_key(&a.name).cmp(&natural_sort_key(&b.name)));

        Ok(entries)
    }

    fn extract_entry(&self, archive_path: &Path, entry_name: &str) -> Result<Bytes, AppError> {
        let file = std::fs::File::open(archive_path)
            .map_err(|e| AppError::InvalidArchive(format!("ファイルを開けません: {e}")))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP を読み取れません: {e}")))?;

        self.extract_from_zip(&mut archive, entry_name)
    }

    /// ZIP を 1 回だけ開いて複数エントリを抽出する
    fn extract_entries(
        &self,
        archive_path: &Path,
        entry_names: &[String],
    ) -> Result<HashMap<String, Bytes>, AppError> {
        let file = std::fs::File::open(archive_path)
            .map_err(|e| AppError::InvalidArchive(format!("ファイルを開けません: {e}")))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP を読み取れません: {e}")))?;

        let mut results = HashMap::with_capacity(entry_names.len());
        for name in entry_names {
            let data = self.extract_from_zip(&mut archive, name)?;
            results.insert(name.clone(), data);
        }
        Ok(results)
    }

    fn supports(&self, path: &Path) -> bool {
        let Some(ext) = path.extension() else {
            return false;
        };
        let ext_lower = format!(".{}", ext.to_string_lossy().to_lowercase());
        ZIP_EXTENSIONS.contains(&ext_lower.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;

    use super::*;
    use crate::config::Settings;

    fn test_validator() -> ArchiveEntryValidator {
        let settings = Settings::from_map(&HashMap::from([(
            "MOUNT_BASE_DIR".to_string(),
            "/tmp".to_string(),
        )]))
        .unwrap();
        ArchiveEntryValidator::new(&settings)
    }

    /// テスト用 ZIP を動的生成するヘルパー
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

    // --- list_entries ---

    #[test]
    fn 正常なzipのエントリ一覧を返す() {
        let reader = ZipArchiveReader::new(test_validator());
        let zip = create_test_zip(&[
            ("image01.jpg", b"fake jpg data"),
            ("image02.png", b"fake png data"),
        ]);

        let entries = reader.list_entries(zip.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "image01.jpg");
        assert_eq!(entries[1].name, "image02.png");
    }

    #[test]
    fn ディレクトリエントリが除外される() {
        let tmp = tempfile::NamedTempFile::with_suffix(".zip").unwrap();
        let mut writer = zip::ZipWriter::new(tmp.as_file().try_clone().unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        // ディレクトリエントリ
        writer
            .add_directory("subdir/", zip::write::SimpleFileOptions::default())
            .unwrap();
        // ファイルエントリ
        writer.start_file("subdir/image.jpg", options).unwrap();
        writer.write_all(b"data").unwrap();
        writer.finish().unwrap();

        let reader = ZipArchiveReader::new(test_validator());
        let entries = reader.list_entries(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "subdir/image.jpg");
    }

    #[test]
    fn 許可されていない拡張子が除外される() {
        let reader = ZipArchiveReader::new(test_validator());
        let zip = create_test_zip(&[
            ("image.jpg", b"ok"),
            ("readme.txt", b"skip"),
            ("program.exe", b"skip"),
        ]);

        let entries = reader.list_entries(zip.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "image.jpg");
    }

    #[test]
    fn エントリが自然順ソートされる() {
        let reader = ZipArchiveReader::new(test_validator());
        let zip = create_test_zip(&[("img10.jpg", b"d"), ("img2.jpg", b"d"), ("img1.jpg", b"d")]);

        let entries = reader.list_entries(zip.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["img1.jpg", "img2.jpg", "img10.jpg"]);
    }

    #[test]
    fn 壊れたzipでinvalid_archiveエラーになる() {
        let tmp = tempfile::NamedTempFile::with_suffix(".zip").unwrap();
        std::fs::write(tmp.path(), b"not a zip file").unwrap();

        let reader = ZipArchiveReader::new(test_validator());
        let result = reader.list_entries(tmp.path());
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("ZIP を読み取れません"));
    }

    // --- extract_entry ---

    #[test]
    fn エントリのバイトデータを正しく抽出する() {
        let reader = ZipArchiveReader::new(test_validator());
        let data = b"hello world image data";
        let zip = create_test_zip(&[("photo.jpg", data)]);

        let result = reader.extract_entry(zip.path(), "photo.jpg").unwrap();
        assert_eq!(&result[..], data);
    }

    #[test]
    fn 存在しないエントリ名でエラーになる() {
        let reader = ZipArchiveReader::new(test_validator());
        let zip = create_test_zip(&[("image.jpg", b"data")]);

        let result = reader.extract_entry(zip.path(), "nonexistent.jpg");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("エントリが見つかりません"));
    }

    #[test]
    fn サイズ上限超過で抽出が中断される() {
        // カスタム設定で小さい上限を設定
        let mut vars = HashMap::from([("MOUNT_BASE_DIR".to_string(), "/tmp".to_string())]);
        vars.insert("ARCHIVE_MAX_ENTRY_SIZE".to_string(), "10".to_string());
        let settings = Settings::from_map(&vars).unwrap();
        let validator = ArchiveEntryValidator::new(&settings);
        let reader = ZipArchiveReader::new(validator);

        let zip = create_test_zip(&[("big.jpg", &[0u8; 100])]);
        let result = reader.extract_entry(zip.path(), "big.jpg");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("サイズ上限を超えました"));
    }

    // --- extract_entries (batch) ---

    #[test]
    fn 複数エントリを一括抽出する() {
        let reader = ZipArchiveReader::new(test_validator());
        let zip = create_test_zip(&[("a.jpg", b"data_a"), ("b.png", b"data_b")]);

        let names = vec!["a.jpg".to_string(), "b.png".to_string()];
        let results = reader.extract_entries(zip.path(), &names).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(&results["a.jpg"][..], b"data_a");
        assert_eq!(&results["b.png"][..], b"data_b");
    }

    // --- supports ---

    #[test]
    fn zip拡張子でtrueを返す() {
        let reader = ZipArchiveReader::new(test_validator());
        assert!(reader.supports(Path::new("archive.zip")));
        assert!(reader.supports(Path::new("archive.ZIP")));
    }

    #[test]
    fn cbz拡張子でtrueを返す() {
        let reader = ZipArchiveReader::new(test_validator());
        assert!(reader.supports(Path::new("comic.cbz")));
    }

    #[test]
    fn rar拡張子でfalseを返す() {
        let reader = ZipArchiveReader::new(test_validator());
        assert!(!reader.supports(Path::new("archive.rar")));
    }
}
