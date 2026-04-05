//! カスタムミドルウェア
//!
//! - `skip_gzip_binary`: バイナリレスポンス (`image/*`, `video/*` 等) の gzip スキップ

pub(crate) mod skip_gzip_binary;
