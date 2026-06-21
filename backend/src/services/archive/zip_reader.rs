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

    /// インデックス指定で ZIP エントリを抽出する (サイズ上限付き)
    ///
    /// `Read::take()` で上限+1 バイトまで読み、超過時はエラーを返す。
    /// `entry_name` はサイズ上限判定とエラーメッセージにのみ使用する。
    fn extract_at_index(
        &self,
        archive: &mut zip::ZipArchive<std::fs::File>,
        index: usize,
        entry_name: &str,
    ) -> Result<Bytes, AppError> {
        let max_size = self.validator.max_entry_size_for(entry_name);

        let file = archive
            .by_index(index)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP エントリ読み取りエラー: {e}")))?;

        let capacity = (file.size()).min(max_size + 1) as usize;
        let mut buf = Vec::with_capacity(capacity);
        file.take(max_size + 1)
            .read_to_end(&mut buf)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP 読み取りエラー: {e}")))?;

        if buf.len() as u64 > max_size {
            return Err(AppError::ArchiveSecurity(format!(
                "抽出時にサイズ上限を超えました: {entry_name}"
            )));
        }

        Ok(Bytes::from(buf))
    }

    /// `list_entries` と同じ規則で「正規化済みエントリ名 → インデックス」対応表を作る
    ///
    /// `zip` クレートの `by_name` は内部マップを生バイトでキーするため、非 UTF-8
    /// (CP437/Shift-JIS) エントリでは CP437 デコード済みの名前と一致せず
    /// `FileNotFound` になる。`list_entries` が返す名前 (CP437 デコード +
    /// バックスラッシュ正規化) でインデックスを引けるよう対応表を構築し、
    /// 抽出はインデックス経由 (`by_index`) で行う。
    fn build_name_index_map(archive: &zip::ZipArchive<std::fs::File>) -> HashMap<String, usize> {
        archive
            .file_names()
            .enumerate()
            .map(|(index, name)| (name.replace('\\', "/"), index))
            .collect()
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
        entries.sort_by_cached_key(|e| natural_sort_key(&e.name));

        Ok(entries)
    }

    fn extract_entry(&self, archive_path: &Path, entry_name: &str) -> Result<Bytes, AppError> {
        // バッチパスに統合: ZIP を1回だけ開いて抽出
        let mut results = self.extract_entries(archive_path, &[entry_name.to_string()])?;
        results.remove(entry_name).ok_or_else(|| {
            AppError::InvalidArchive(format!("エントリが見つかりません: {entry_name}"))
        })
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

        // 非 UTF-8 エントリ対応のため名前→インデックス対応表を一度だけ構築する
        let index_map = Self::build_name_index_map(&archive);

        let mut results = HashMap::with_capacity(entry_names.len());
        for name in entry_names {
            let index = index_map.get(name).copied().ok_or_else(|| {
                AppError::InvalidArchive(format!("エントリが見つかりません: {name}"))
            })?;
            let data = self.extract_at_index(&mut archive, index, name)?;
            results.insert(name.clone(), data);
        }
        Ok(results)
    }

    /// ZIP エントリをファイルにストリーミング展開する (メモリに全展開しない)
    fn extract_entry_to_file(
        &self,
        archive_path: &Path,
        entry_name: &str,
        dest: &Path,
    ) -> Result<(), AppError> {
        let max_size = self.validator.max_entry_size_for(entry_name);
        let file = std::fs::File::open(archive_path)
            .map_err(|e| AppError::InvalidArchive(format!("ファイルを開けません: {e}")))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP を読み取れません: {e}")))?;

        // 非 UTF-8 エントリ対応のためインデックス経由で抽出する
        let index = Self::build_name_index_map(&archive)
            .get(entry_name)
            .copied()
            .ok_or_else(|| {
                AppError::InvalidArchive(format!("エントリが見つかりません: {entry_name}"))
            })?;
        let mut entry = archive
            .by_index(index)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP エントリ読み取りエラー: {e}")))?;

        let mut dest_file = std::fs::File::create(dest)
            .map_err(|e| AppError::InvalidArchive(format!("ファイル作成エラー: {e}")))?;
        let mut chunk = vec![0u8; EXTRACT_CHUNK_SIZE];
        let mut total: u64 = 0;

        loop {
            let n = entry
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
            std::io::Write::write_all(&mut dest_file, &chunk[..n])
                .map_err(|e| AppError::InvalidArchive(format!("書き込みエラー: {e}")))?;
        }

        Ok(())
    }

    fn supports(&self, path: &Path) -> bool {
        let Some(ext) = path.extension() else {
            return false;
        };
        let ext_lower = format!(".{}", ext.to_string_lossy().to_lowercase());
        ZIP_EXTENSIONS.contains(&ext_lower.as_str())
    }

    /// サムネイル用: 最初の画像エントリで即座に返す (全エントリ走査・合計サイズ検証なし)
    fn find_first_image(&self, archive_path: &Path) -> Result<Option<ArchiveEntry>, AppError> {
        let file = std::fs::File::open(archive_path)
            .map_err(|e| AppError::InvalidArchive(format!("ファイルを開けません: {e}")))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::InvalidArchive(format!("ZIP を読み取れません: {e}")))?;

        for i in 0..archive.len() {
            let info = archive.by_index_raw(i).map_err(|e| {
                AppError::InvalidArchive(format!("ZIP エントリ読み取りエラー: {e}"))
            })?;

            if info.encrypted() {
                return Err(AppError::ArchivePassword(
                    "パスワード付きアーカイブは未対応です".to_string(),
                ));
            }

            if info.is_dir() {
                continue;
            }

            let name = info.name().replace('\\', "/");

            if ArchiveEntryValidator::validate_entry_name(&name).is_err() {
                continue;
            }

            // 画像エントリが見つかったら即座に返す (合計サイズ検証スキップ)
            if super::reader::is_image_name(&name) {
                return Ok(Some(ArchiveEntry {
                    name,
                    size_compressed: info.compressed_size(),
                    size_uncompressed: info.size(),
                    is_dir: false,
                }));
            }
        }

        Ok(None)
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

    /// CRC32 (IEEE) を計算する (手組み ZIP フィクスチャ用)
    fn crc32_ieee(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// 生バイトのエントリ名 (UTF-8 フラグ無し) を持つ STORED ZIP を手組みする
    ///
    /// `zip` クレートの writer は非 ASCII 名に必ず UTF-8 フラグを立てるため、
    /// 日本の Windows が生成する Shift-JIS (UTF-8 フラグ無し) ZIP を writer では
    /// 再現できない。最小限の ZIP 構造を直接書き出してフィクスチャを作る。
    fn create_raw_name_zip(raw_name: &[u8], data: &[u8]) -> tempfile::NamedTempFile {
        let crc = crc32_ieee(data);
        let name_len = u16::try_from(raw_name.len()).unwrap();
        let size = u32::try_from(data.len()).unwrap();
        let mut bytes = Vec::new();

        // ローカルファイルヘッダ
        bytes.extend_from_slice(&0x0403_4b50u32.to_le_bytes()); // signature
        bytes.extend_from_slice(&20u16.to_le_bytes()); // version needed
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags (UTF-8 フラグ無し)
        bytes.extend_from_slice(&0u16.to_le_bytes()); // compression: stored
        bytes.extend_from_slice(&0u16.to_le_bytes()); // mod time
        bytes.extend_from_slice(&0u16.to_le_bytes()); // mod date
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes()); // compressed size
        bytes.extend_from_slice(&size.to_le_bytes()); // uncompressed size
        bytes.extend_from_slice(&name_len.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes()); // extra length
        bytes.extend_from_slice(raw_name);
        bytes.extend_from_slice(data);

        let cd_offset = u32::try_from(bytes.len()).unwrap();

        // セントラルディレクトリヘッダ
        bytes.extend_from_slice(&0x0201_4b50u32.to_le_bytes()); // signature
        bytes.extend_from_slice(&20u16.to_le_bytes()); // version made by
        bytes.extend_from_slice(&20u16.to_le_bytes()); // version needed
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags (UTF-8 フラグ無し)
        bytes.extend_from_slice(&0u16.to_le_bytes()); // compression
        bytes.extend_from_slice(&0u16.to_le_bytes()); // mod time
        bytes.extend_from_slice(&0u16.to_le_bytes()); // mod date
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&name_len.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes()); // extra length
        bytes.extend_from_slice(&0u16.to_le_bytes()); // comment length
        bytes.extend_from_slice(&0u16.to_le_bytes()); // disk number start
        bytes.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        bytes.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        bytes.extend_from_slice(&0u32.to_le_bytes()); // local header offset
        bytes.extend_from_slice(raw_name);

        let cd_size = u32::try_from(bytes.len()).unwrap() - cd_offset;

        // End of Central Directory
        bytes.extend_from_slice(&0x0605_4b50u32.to_le_bytes()); // signature
        bytes.extend_from_slice(&0u16.to_le_bytes()); // disk number
        bytes.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
        bytes.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
        bytes.extend_from_slice(&1u16.to_le_bytes()); // total entries
        bytes.extend_from_slice(&cd_size.to_le_bytes());
        bytes.extend_from_slice(&cd_offset.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes()); // comment length

        let tmp = tempfile::NamedTempFile::with_suffix(".zip").unwrap();
        std::fs::write(tmp.path(), &bytes).unwrap();
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
        let err = result.unwrap_err().to_string();
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
        let err = result.unwrap_err().to_string();
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
        let err = result.unwrap_err().to_string();
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

    // --- 非 UTF-8 (Shift-JIS) エントリ名 ---

    #[test]
    fn shift_jis名の非utf8エントリを抽出できる() {
        // 日本の Windows 製 ZIP を再現:
        // "その他/img.jpg" を Shift-JIS で格納し UTF-8 フラグ無し
        // その他 = 0x82bb 0x82cc 0x91bc (0x5C を含まない)
        let raw_name: &[u8] = b"\x82\xbb\x82\xcc\x91\xbc/img.jpg";
        let data = b"jpeg image bytes";
        let zip = create_raw_name_zip(raw_name, data);
        let reader = ZipArchiveReader::new(test_validator());

        // list_entries が返す名前 (zip クレートは非 UTF-8 名を CP437 でデコードする)
        let entries = reader.list_entries(zip.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let listed_name = entries[0].name.clone();

        // list_entries と同じ名前で抽出できること
        // (修正前は by_name が生バイトと突き合わせるため FileNotFound で失敗する)
        let extracted = reader.extract_entry(zip.path(), &listed_name).unwrap();
        assert_eq!(&extracted[..], data);
    }

    #[test]
    fn shift_jis名のエントリをバッチ抽出できる() {
        let raw_name: &[u8] = b"\x82\xbb\x82\xcc\x91\xbc/img.jpg";
        let data = b"batch jpeg bytes";
        let zip = create_raw_name_zip(raw_name, data);
        let reader = ZipArchiveReader::new(test_validator());

        let listed_name = reader.list_entries(zip.path()).unwrap()[0].name.clone();
        let results = reader
            .extract_entries(zip.path(), std::slice::from_ref(&listed_name))
            .unwrap();
        assert_eq!(&results[&listed_name][..], data);
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

    // --- extract_entry_to_file ---

    #[test]
    fn extract_entry_to_fileがファイルにストリーミング展開する() {
        let reader = ZipArchiveReader::new(test_validator());
        let data = b"streaming test data for video";
        let zip = create_test_zip(&[("video.mp4", data)]);

        let dest = tempfile::NamedTempFile::new().unwrap();
        reader
            .extract_entry_to_file(zip.path(), "video.mp4", dest.path())
            .unwrap();

        let written = std::fs::read(dest.path()).unwrap();
        assert_eq!(&written[..], data);
    }

    #[test]
    fn extract_entry_to_fileのサイズ上限超過でエラーになる() {
        let mut vars = HashMap::from([("MOUNT_BASE_DIR".to_string(), "/tmp".to_string())]);
        vars.insert("ARCHIVE_MAX_VIDEO_ENTRY_SIZE".to_string(), "10".to_string());
        let settings = Settings::from_map(&vars).unwrap();
        let validator = ArchiveEntryValidator::new(&settings);
        let reader = ZipArchiveReader::new(validator);

        let zip = create_test_zip(&[("big.mp4", &[0u8; 100])]);
        let dest = tempfile::NamedTempFile::new().unwrap();
        let result = reader.extract_entry_to_file(zip.path(), "big.mp4", dest.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("サイズ上限を超えました"));
    }
}
