//! RAR/CBR アーカイブリーダー
//!
//! `unrar-free` CLI を subprocess で呼び出して RAR/CBR を読み取る。
//! CLI 安全性: `Command::arg()` のみ使用 (shell 経由禁止)、`--` でオプション終端。

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

/// RAR 拡張子
const RAR_EXTENSIONS: &[&str] = &[".rar", ".cbr"];

/// RAR/CBR アーカイブリーダー (unrar-free CLI)
pub(crate) struct RarArchiveReader {
    validator: ArchiveEntryValidator,
    is_available: bool,
}

impl RarArchiveReader {
    pub(crate) fn new(validator: ArchiveEntryValidator) -> Self {
        let is_available = check_unrar_available();
        Self {
            validator,
            is_available,
        }
    }

    pub(crate) fn is_available(&self) -> bool {
        self.is_available
    }
}

/// `unrar-free` がインストールされているか確認する
fn check_unrar_available() -> bool {
    Command::new("which")
        .arg("unrar-free")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `unrar-free l` の出力からエントリ情報をパースする
///
/// unrar-free の出力形式 (v モード):
/// ```text
///  Name            Size  Packed  Ratio  Date       Time   Attr  CRC  Meth  Ver
/// ---------------------------------------------------------------------------
///  image01.jpg      1234    500   41%   2024-01-01 00:00 .....A  ABCDEF  m3b 2.9
/// ---------------------------------------------------------------------------
/// ```
fn parse_unrar_list(output: &str) -> Vec<(String, u64, u64, bool)> {
    let mut entries = Vec::new();
    let mut in_body = false;
    let mut separator_count = 0;

    for line in output.lines() {
        let trimmed = line.trim();

        // セパレータ行 (---) をカウント
        if trimmed.starts_with("----") {
            separator_count += 1;
            if separator_count == 1 {
                in_body = true;
            } else {
                // 2つ目のセパレータでボディ終了
                break;
            }
            continue;
        }

        if !in_body || trimmed.is_empty() {
            continue;
        }

        // 各行をパース: Name  Size  Packed  Ratio  Date  Time  Attr  CRC  Meth  Ver
        // Attr は 7番目のフィールド (index 6)
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 7 {
            continue;
        }

        let name = parts[0].to_string();
        let size_uncompressed = parts[1].parse::<u64>().unwrap_or(0);
        let size_compressed = parts[2].parse::<u64>().unwrap_or(0);
        // Attr は index 6: "D....." (dir) or ".....A" (file) パターン
        let is_dir = parts
            .get(6)
            .is_some_and(|attr| attr.starts_with('D') || attr.starts_with('d'));

        entries.push((name, size_compressed, size_uncompressed, is_dir));
    }

    entries
}

impl ArchiveReader for RarArchiveReader {
    fn list_entries(&self, archive_path: &Path) -> Result<Vec<ArchiveEntry>, AppError> {
        if !self.is_available {
            return Err(AppError::InvalidArchive(
                "unrar-free がインストールされていません".to_string(),
            ));
        }

        // unrar-free v で詳細一覧を取得 (-- でオプション終端)
        let output = Command::new("unrar-free")
            .arg("v")
            .arg("--")
            .arg(archive_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| AppError::InvalidArchive(format!("unrar-free 実行エラー: {e}")))?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        // パスワード検出
        if stderr.contains("password")
            || stderr.contains("encrypted")
            || stderr.contains("wrong password")
        {
            return Err(AppError::ArchivePassword(
                "パスワード付きアーカイブは未対応です".to_string(),
            ));
        }

        if !output.status.success() {
            return Err(AppError::InvalidArchive(format!(
                "unrar-free エラー (exit {}): {}",
                output.status,
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let raw_entries = parse_unrar_list(&stdout);

        let mut entries = Vec::new();
        let mut total_uncompressed: u64 = 0;

        for (name, size_compressed, size_uncompressed, is_dir) in raw_entries {
            if is_dir {
                continue;
            }

            // バックスラッシュ正規化
            let name = name.replace('\\', "/");

            // セキュリティ検証 (不正エントリはスキップ)
            if ArchiveEntryValidator::validate_entry_name(&name).is_err() {
                continue;
            }
            if !ArchiveEntryValidator::is_allowed_extension(&name) {
                continue;
            }
            if self
                .validator
                .validate_entry_size(size_compressed, size_uncompressed, &name)
                .is_err()
            {
                continue;
            }

            total_uncompressed += size_uncompressed;
            entries.push(ArchiveEntry {
                name,
                size_compressed,
                size_uncompressed,
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
                "unrar-free がインストールされていません".to_string(),
            ));
        }

        let max_size = self.validator.max_entry_size_for(entry_name);

        // unrar-free p -inul で stdout にバイナリ出力 (-- でオプション終端)
        let mut child = Command::new("unrar-free")
            .arg("p")
            .arg("-inul")
            .arg("--")
            .arg(archive_path)
            .arg(entry_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::InvalidArchive(format!("unrar-free 実行エラー: {e}")))?;

        // チャンク読みでサイズ上限チェック
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::InvalidArchive("unrar-free stdout 取得失敗".to_string()))?;

        let mut reader = std::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        let mut chunk = vec![0u8; EXTRACT_CHUNK_SIZE];
        let mut total: u64 = 0;

        loop {
            let n = reader
                .read(&mut chunk)
                .map_err(|e| AppError::InvalidArchive(format!("unrar-free 読み取りエラー: {e}")))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > max_size {
                // サイズ上限超過 — プロセスを kill
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
            .map_err(|e| AppError::InvalidArchive(format!("unrar-free 待機エラー: {e}")))?;
        if !status.success() && buf.is_empty() {
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
        RAR_EXTENSIONS.contains(&ext_lower.as_str())
    }

    /// サムネイル用: 最初の画像エントリで即座に返す (合計サイズ検証・ソートなし)
    fn find_first_image(&self, archive_path: &Path) -> Result<Option<ArchiveEntry>, AppError> {
        if !self.is_available {
            return Err(AppError::InvalidArchive(
                "unrar-free がインストールされていません".to_string(),
            ));
        }

        let output = Command::new("unrar-free")
            .arg("v")
            .arg("--")
            .arg(archive_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| AppError::InvalidArchive(format!("unrar-free 実行エラー: {e}")))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("password")
            || stderr.contains("encrypted")
            || stderr.contains("wrong password")
        {
            return Err(AppError::ArchivePassword(
                "パスワード付きアーカイブは未対応です".to_string(),
            ));
        }
        if !output.status.success() {
            return Err(AppError::InvalidArchive(format!(
                "unrar-free エラー (exit {}): {}",
                output.status,
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(find_first_image_from_unrar_output(&stdout))
    }
}

/// `unrar-free v` の出力から最初の画像エントリを探す (早期リターン)
fn find_first_image_from_unrar_output(output: &str) -> Option<ArchiveEntry> {
    let mut in_body = false;
    let mut separator_count = 0;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("----") {
            separator_count += 1;
            if separator_count == 1 {
                in_body = true;
            } else {
                break;
            }
            continue;
        }

        if !in_body || trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 7 {
            continue;
        }

        // ディレクトリはスキップ
        let is_dir = parts
            .get(6)
            .is_some_and(|attr| attr.starts_with('D') || attr.starts_with('d'));
        if is_dir {
            continue;
        }

        let name = parts[0].replace('\\', "/");
        if ArchiveEntryValidator::validate_entry_name(&name).is_err() {
            continue;
        }

        // 画像エントリが見つかったら即座に返す
        if super::reader::is_image_name(&name) {
            let size_uncompressed = parts[1].parse::<u64>().unwrap_or(0);
            let size_compressed = parts[2].parse::<u64>().unwrap_or(0);
            return Some(ArchiveEntry {
                name,
                size_compressed,
                size_uncompressed,
                is_dir: false,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn unrar_free未インストール時にsupportsがfalseを返す() {
        // is_available=false を直接設定
        let validator = ArchiveEntryValidator::new(
            &crate::config::Settings::from_map(&std::collections::HashMap::from([(
                "MOUNT_BASE_DIR".to_string(),
                "/tmp".to_string(),
            )]))
            .unwrap(),
        );
        let reader = RarArchiveReader {
            validator,
            is_available: false,
        };
        assert!(!reader.supports(Path::new("test.rar")));
    }

    #[test]
    fn parse_unrar_listがエントリをパースできる() {
        let output = "\
UNRAR-FREE 0.1.2

Archive: test.rar

 Name            Size  Packed  Ratio  Date       Time   Attr  CRC  Meth  Ver
---------------------------------------------------------------------------
 image01.jpg      1234    500   41%   2024-01-01 00:00 .....A  ABCDEF  m3b 2.9
 subdir/            0      0    0%   2024-01-01 00:00 D.....  000000  m0  2.9
---------------------------------------------------------------------------
";
        let entries = parse_unrar_list(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "image01.jpg");
        assert_eq!(entries[0].1, 500); // compressed
        assert_eq!(entries[0].2, 1234); // uncompressed
        assert!(!entries[0].3); // not dir
        assert!(entries[1].3); // dir
    }

    #[test]
    fn find_first_imageが最初の画像エントリを返す() {
        let output = "\
UNRAR-FREE 0.1.2

Archive: test.rar

 Name            Size  Packed  Ratio  Date       Time   Attr  CRC  Meth  Ver
---------------------------------------------------------------------------
 subdir/            0      0    0%   2024-01-01 00:00 D.....  000000  m0  2.9
 readme.txt       100     50   50%   2024-01-01 00:00 .....A  AAAAAA  m3b 2.9
 image01.jpg     1234    500   41%   2024-01-01 00:00 .....A  ABCDEF  m3b 2.9
 image02.png     5678   2000   35%   2024-01-01 00:00 .....A  123456  m3b 2.9
---------------------------------------------------------------------------
";
        let entry = find_first_image_from_unrar_output(output).expect("画像エントリが見つかるべき");
        assert_eq!(entry.name, "image01.jpg");
        assert_eq!(entry.size_compressed, 500);
        assert_eq!(entry.size_uncompressed, 1234);
    }

    #[test]
    fn find_first_imageが画像なしでnoneを返す() {
        let output = "\
UNRAR-FREE 0.1.2

Archive: test.rar

 Name            Size  Packed  Ratio  Date       Time   Attr  CRC  Meth  Ver
---------------------------------------------------------------------------
 readme.txt       100     50   50%   2024-01-01 00:00 .....A  AAAAAA  m3b 2.9
 data.csv        2000   1000   50%   2024-01-01 00:00 .....A  BBBBBB  m3b 2.9
---------------------------------------------------------------------------
";
        let result = find_first_image_from_unrar_output(output);
        assert!(result.is_none());
    }
}
