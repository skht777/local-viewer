//! 7z アーカイブリーダー
//!
//! `7z` CLI (p7zip-full) を subprocess で呼び出して 7z を読み取る。
//! CLI 安全性: `Command::arg()` のみ使用 (shell 経由禁止)、`--` でオプション終端。

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use bytes::Bytes;

use super::reader::{ArchiveEntry, ArchiveReader};
use super::security::ArchiveEntryValidator;
use crate::errors::AppError;
use crate::services::natural_sort::natural_sort_key;

/// チャンク読みサイズ (64KiB)
const EXTRACT_CHUNK_SIZE: usize = 64 * 1024;

/// 7z 拡張子
const SEVENZ_EXTENSIONS: &[&str] = &[".7z"];

/// 7z アーカイブリーダー (p7zip CLI)
pub(crate) struct SevenZipArchiveReader {
    validator: ArchiveEntryValidator,
    is_available: bool,
}

impl SevenZipArchiveReader {
    pub(crate) fn new(validator: ArchiveEntryValidator) -> Self {
        let is_available = check_7z_available();
        Self {
            validator,
            is_available,
        }
    }

    pub(crate) fn is_available(&self) -> bool {
        self.is_available
    }
}

/// `7z` がインストールされているか確認する
fn check_7z_available() -> bool {
    Command::new("which")
        .arg("7z")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `7z t` でパスワード保護を検出する
fn check_password(archive_path: &Path) -> Result<(), AppError> {
    let output = Command::new("7z")
        .arg("t")
        .arg("--")
        .arg(archive_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| AppError::InvalidArchive(format!("7z 実行エラー: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("wrong password") || stderr.contains("cannot open encrypted") {
            return Err(AppError::ArchivePassword(
                "パスワード付きアーカイブは未対応です".to_string(),
            ));
        }
    }
    Ok(())
}

/// `7z l -slt` の出力を Key=Value ブロックのリストにパースする
///
/// 出力形式:
/// ```text
/// Path = dir/image01.jpg
/// Folder = -
/// Size = 1234
/// Packed Size = 500
///
/// Path = dir/image02.jpg
/// ...
/// ```
fn parse_slt_blocks(output: &str) -> Vec<HashMap<String, String>> {
    let mut blocks = Vec::new();
    let mut current = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                blocks.push(current);
                current = HashMap::new();
            }
            continue;
        }
        // "Key = Value" 形式。空値の場合 "Key =" (末尾スペースなし) になることがある
        if let Some((key, value)) = trimmed.split_once(" = ") {
            current.insert(key.to_string(), value.to_string());
        } else if let Some(key) = trimmed.strip_suffix('=') {
            let key = key.trim_end();
            current.insert(key.to_string(), String::new());
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

impl ArchiveReader for SevenZipArchiveReader {
    fn list_entries(&self, archive_path: &Path) -> Result<Vec<ArchiveEntry>, AppError> {
        if !self.is_available {
            return Err(AppError::InvalidArchive(
                "7z がインストールされていません".to_string(),
            ));
        }

        // パスワード検出
        check_password(archive_path)?;

        // 7z l -slt で Key=Value 形式の一覧を取得
        let output = Command::new("7z")
            .arg("l")
            .arg("-slt")
            .arg("--")
            .arg(archive_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| AppError::InvalidArchive(format!("7z 実行エラー: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::InvalidArchive(format!(
                "7z l エラー: {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let blocks = parse_slt_blocks(&stdout);

        let mut entries = Vec::new();
        let mut total_uncompressed: u64 = 0;

        for block in &blocks {
            let Some(path) = block.get("Path") else {
                continue;
            };
            if path.is_empty() {
                continue;
            }
            // ディレクトリをスキップ
            if block.get("Folder").is_some_and(|v| v == "+") {
                continue;
            }

            // バックスラッシュ正規化
            let name = path.replace('\\', "/");

            // セキュリティ検証
            if ArchiveEntryValidator::validate_entry_name(&name).is_err() {
                continue;
            }
            if !ArchiveEntryValidator::is_allowed_extension(&name) {
                continue;
            }

            // ソリッドアーカイブでは Packed Size が空文字列
            let compressed: u64 = block
                .get("Packed Size")
                .and_then(|v| if v.is_empty() { None } else { v.parse().ok() })
                .unwrap_or(0);
            let uncompressed: u64 = block
                .get("Size")
                .and_then(|v| if v.is_empty() { None } else { v.parse().ok() })
                .unwrap_or(0);

            if self
                .validator
                .validate_entry_size(compressed, uncompressed, &name)
                .is_err()
            {
                continue;
            }
            total_uncompressed += uncompressed;

            entries.push(ArchiveEntry {
                name,
                size_compressed: compressed,
                size_uncompressed: uncompressed,
                is_dir: false,
            });
        }

        self.validator.validate_total_size(total_uncompressed)?;
        entries.sort_by_cached_key(|e| natural_sort_key(&e.name));
        Ok(entries)
    }

    fn extract_entry(&self, archive_path: &Path, entry_name: &str) -> Result<Bytes, AppError> {
        if !self.is_available {
            return Err(AppError::InvalidArchive(
                "7z がインストールされていません".to_string(),
            ));
        }

        let max_size = self.validator.max_entry_size_for(entry_name);

        // 7z x -so で stdout にバイナリ出力
        let mut child = Command::new("7z")
            .arg("x")
            .arg("-so")
            .arg("--")
            .arg(archive_path)
            .arg(entry_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::InvalidArchive(format!("7z 実行エラー: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::InvalidArchive("7z stdout 取得失敗".to_string()))?;

        let mut reader = std::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        let mut chunk = vec![0u8; EXTRACT_CHUNK_SIZE];
        let mut total: u64 = 0;

        loop {
            let n = reader
                .read(&mut chunk)
                .map_err(|e| AppError::InvalidArchive(format!("7z 読み取りエラー: {e}")))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > max_size {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::ArchiveSecurity(format!(
                    "抽出時にサイズ上限を超えました: {entry_name}"
                )));
            }
            buf.extend_from_slice(&chunk[..n]);
        }

        let status = child
            .wait()
            .map_err(|e| AppError::InvalidArchive(format!("7z 待機エラー: {e}")))?;

        if !status.success() && buf.is_empty() {
            // stderr を確認してエントリ未発見を判定
            return Err(AppError::InvalidArchive(format!(
                "エントリが見つかりません: {entry_name}"
            )));
        }

        Ok(Bytes::from(buf))
    }

    fn supports(&self, path: &Path) -> bool {
        if !self.is_available {
            return false;
        }
        let Some(ext) = path.extension() else {
            return false;
        };
        let ext_lower = format!(".{}", ext.to_string_lossy().to_lowercase());
        SEVENZ_EXTENSIONS.contains(&ext_lower.as_str())
    }

    /// サムネイル用: 最初の画像エントリで即座に返す (パスワード検査・合計サイズ検証・ソートなし)
    ///
    /// `check_password()` をスキップする。`7z l` はパスワード有無に関わらずメタデータ取得可能。
    /// パスワード付きアーカイブでは本メソッドは成功するが、後続の `extract_entry` でエラーになる。
    /// サムネイル用途では graceful degradation として許容（サムネなし表示）。
    fn find_first_image(&self, archive_path: &Path) -> Result<Option<ArchiveEntry>, AppError> {
        if !self.is_available {
            return Err(AppError::InvalidArchive(
                "7z がインストールされていません".to_string(),
            ));
        }

        let output = Command::new("7z")
            .arg("l")
            .arg("-slt")
            .arg("--")
            .arg(archive_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| AppError::InvalidArchive(format!("7z 実行エラー: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::InvalidArchive(format!(
                "7z l エラー: {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(find_first_image_from_slt_output(&stdout))
    }
}

/// `7z l -slt` の出力から最初の画像エントリを探す (早期リターン)
fn find_first_image_from_slt_output(output: &str) -> Option<ArchiveEntry> {
    let mut current: HashMap<String, String> = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if let Some(entry) = try_build_image_entry(&current) {
                return Some(entry);
            }
            current.clear();
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(" = ") {
            current.insert(key.to_string(), value.to_string());
        } else if let Some(key) = trimmed.strip_suffix('=') {
            let key = key.trim_end();
            current.insert(key.to_string(), String::new());
        }
    }

    // 末尾ブロック (最終行が空行でない場合)
    if !current.is_empty()
        && let Some(entry) = try_build_image_entry(&current)
    {
        return Some(entry);
    }

    None
}

/// Key=Value ブロックから画像エントリを構築する (画像でなければ None)
fn try_build_image_entry(block: &HashMap<String, String>) -> Option<ArchiveEntry> {
    let path = block.get("Path")?;
    if path.is_empty() {
        return None;
    }

    // ディレクトリをスキップ
    if block.get("Folder").is_some_and(|v| v == "+") {
        return None;
    }

    let name = path.replace('\\', "/");
    if ArchiveEntryValidator::validate_entry_name(&name).is_err() {
        return None;
    }

    if !super::reader::is_image_name(&name) {
        return None;
    }

    let compressed: u64 = block
        .get("Packed Size")
        .and_then(|v| if v.is_empty() { None } else { v.parse().ok() })
        .unwrap_or(0);
    let uncompressed: u64 = block
        .get("Size")
        .and_then(|v| if v.is_empty() { None } else { v.parse().ok() })
        .unwrap_or(0);

    Some(ArchiveEntry {
        name,
        size_compressed: compressed,
        size_uncompressed: uncompressed,
        is_dir: false,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn sevenz未インストール時にsupportsがfalseを返す() {
        let validator = ArchiveEntryValidator::new(
            &crate::config::Settings::from_map(&std::collections::HashMap::from([(
                "MOUNT_BASE_DIR".to_string(),
                "/tmp".to_string(),
            )]))
            .unwrap(),
        );
        let reader = SevenZipArchiveReader {
            validator,
            is_available: false,
        };
        assert!(!reader.supports(Path::new("test.7z")));
    }

    #[test]
    fn parse_slt_blocksがkey_valueブロックをパースできる() {
        let output = "\
Path = image01.jpg
Folder = -
Size = 1234
Packed Size = 500

Path = subdir
Folder = +
Size = 0
Packed Size = 0

Path = image02.png
Folder = -
Size = 5678
Packed Size =
";
        let blocks = parse_slt_blocks(output);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].get("Path").unwrap(), "image01.jpg");
        assert_eq!(blocks[0].get("Size").unwrap(), "1234");
        assert_eq!(blocks[1].get("Folder").unwrap(), "+");
        // ソリッドアーカイブ: Packed Size が空
        assert_eq!(blocks[2].get("Packed Size").unwrap(), "");
    }

    #[test]
    fn find_first_imageが最初の画像エントリを返す() {
        let output = "\
Path = readme.txt
Folder = -
Size = 100
Packed Size = 50

Path = subdir
Folder = +
Size = 0
Packed Size = 0

Path = image01.jpg
Folder = -
Size = 1234
Packed Size = 500

Path = image02.png
Folder = -
Size = 5678
Packed Size = 2000
";
        let result = find_first_image_from_slt_output(output);
        let entry = result.expect("画像エントリが見つかるべき");
        assert_eq!(entry.name, "image01.jpg");
        assert_eq!(entry.size_compressed, 500);
        assert_eq!(entry.size_uncompressed, 1234);
    }

    #[test]
    fn find_first_imageが画像なしでnoneを返す() {
        let output = "\
Path = readme.txt
Folder = -
Size = 100
Packed Size = 50

Path = data.csv
Folder = -
Size = 2000
Packed Size = 1000
";
        let result = find_first_image_from_slt_output(output);
        assert!(result.is_none());
    }

    #[test]
    fn find_first_imageがソリッドアーカイブの画像を返す() {
        let output = "\
Path = image01.webp
Folder = -
Size = 3000
Packed Size =
";
        let result = find_first_image_from_slt_output(output);
        let entry = result.expect("画像エントリが見つかるべき");
        assert_eq!(entry.name, "image01.webp");
        assert_eq!(entry.size_compressed, 0);
        assert_eq!(entry.size_uncompressed, 3000);
    }
}
