//! アーカイブサービス
//!
//! ZIP/RAR/7z の統一インターフェース + moka キャッシュ。

pub(crate) mod rar_reader;
pub(crate) mod reader;
pub(crate) mod security;
pub(crate) mod sevenz_reader;
pub(crate) mod zip_reader;
