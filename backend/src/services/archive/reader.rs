//! アーカイブリーダートレイト + エントリ型定義
//!
//! ZIP/RAR/7z リーダーの共通インターフェース。
//! 全リーダーは `Send + Sync` を満たし、`spawn_blocking` 内で安全に使用可能。

use std::collections::HashMap;
use std::path::Path;

use bytes::Bytes;

use crate::errors::AppError;

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

    /// 指定パスのアーカイブ形式をサポートするか
    fn supports(&self, path: &Path) -> bool;
}
