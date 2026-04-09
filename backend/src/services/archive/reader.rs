//! アーカイブリーダートレイト + エントリ型定義
//!
//! ZIP/RAR/7z リーダーの共通インターフェース。
//! 全リーダーは `Send + Sync` を満たし、`spawn_blocking` 内で安全に使用可能。

use std::collections::HashMap;
use std::path::Path;

use bytes::Bytes;

use crate::errors::AppError;
use crate::services::extensions::IMAGE_EXTENSIONS;

/// エントリ名が画像拡張子を持つかチェックする
pub(crate) fn is_image_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// アーカイブ内のエントリ情報
#[derive(Debug, Clone)]
pub(crate) struct ArchiveEntry {
    /// エントリのフルパス (例: "dir/image01.jpg")
    pub name: String,
    /// 圧縮後サイズ (bytes)
    pub size_compressed: u64,
    /// 展開後サイズ (bytes)
    pub size_uncompressed: u64,
    /// ディレクトリエントリかどうか
    pub is_dir: bool,
}

/// アーカイブリーダーの共通インターフェース
///
/// - `list_entries`: エントリ一覧を返す (セキュリティ検証 + 拡張子フィルタ済み)
/// - `extract_entry`: 1エントリをバイト列として抽出する
/// - `extract_entries`: 複数エントリを一括抽出する (デフォルト実装あり)
/// - `supports`: 指定パスのアーカイブ形式をサポートするか
pub(crate) trait ArchiveReader: Send + Sync {
    /// セキュリティ検証・拡張子フィルタ・自然順ソート済みのエントリ一覧を返す
    fn list_entries(&self, archive_path: &Path) -> Result<Vec<ArchiveEntry>, AppError>;

    /// 1エントリをバイト列として抽出する
    ///
    /// サイズ上限を超えた場合は中断して `ArchiveSecurity` エラーを返す。
    fn extract_entry(&self, archive_path: &Path, entry_name: &str) -> Result<Bytes, AppError>;

    /// 複数エントリを一括抽出する
    ///
    /// デフォルト実装は `extract_entry` をループ呼び出しする。
    /// ZIP リーダーはアーカイブを 1 回だけ開くオーバーライドを提供する。
    fn extract_entries(
        &self,
        archive_path: &Path,
        entry_names: &[String],
    ) -> Result<HashMap<String, Bytes>, AppError> {
        let mut results = HashMap::with_capacity(entry_names.len());
        for name in entry_names {
            let data = self.extract_entry(archive_path, name)?;
            results.insert(name.clone(), data);
        }
        Ok(results)
    }

    /// 1 エントリをファイルに直接展開する
    ///
    /// デフォルト実装は `extract_entry` でメモリに読み込み、ファイルに書き出す。
    /// 大きなエントリではリーダー固有のストリーミング実装でオーバーライド可能。
    fn extract_entry_to_file(
        &self,
        archive_path: &Path,
        entry_name: &str,
        dest: &Path,
    ) -> Result<(), AppError> {
        let data = self.extract_entry(archive_path, entry_name)?;
        std::fs::write(dest, &data)
            .map_err(|e| AppError::InvalidArchive(format!("ファイル書き込みエラー: {e}")))?;
        Ok(())
    }

    /// 指定パスのアーカイブ形式をサポートするか
    fn supports(&self, path: &Path) -> bool;

    /// サムネイル用: 最初の画像エントリを高速に探す
    ///
    /// `list_entries` と異なり全エントリ走査・合計サイズ検証を行わず、
    /// 最初の画像エントリが見つかった時点で即座に返す。
    /// デフォルト実装は `list_entries` にフォールバックする。
    fn find_first_image(&self, archive_path: &Path) -> Result<Option<ArchiveEntry>, AppError> {
        let entries = self.list_entries(archive_path)?;
        Ok(entries
            .into_iter()
            .find(|e| !e.is_dir && is_image_name(&e.name)))
    }
}
