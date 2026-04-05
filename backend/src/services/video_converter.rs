//! 動画変換サービス
//!
//! `FFmpeg` subprocess で動画フレーム抽出と MKV→MP4 remux を行う。
//! `spawn_blocking` 内から呼ぶこと (同期 I/O)。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::Settings;
use crate::services::extensions::REMUX_EXTENSIONS;
use crate::services::temp_file_cache::TempFileCache;

/// 動画変換サービス
///
/// - `extract_frame`: 動画から 1 フレームを JPEG として抽出
/// - `get_remuxed`: MKV → MP4 remux (ストリーム再パッケージ、エンコードなし)
/// - `FFmpeg` が見つからない場合は `is_available = false`
pub(crate) struct VideoConverter {
    cache: Arc<TempFileCache>,
    ffmpeg_path: Option<PathBuf>,
    remux_timeout: u64,
    thumb_seek_seconds: u64,
    thumb_timeout: u64,
}

impl VideoConverter {
    pub(crate) fn new(cache: Arc<TempFileCache>, settings: &Settings) -> Self {
        let ffmpeg_path = find_ffmpeg();
        Self {
            cache,
            ffmpeg_path,
            remux_timeout: settings.video_remux_timeout,
            thumb_seek_seconds: settings.video_thumb_seek_seconds,
            thumb_timeout: settings.video_thumb_timeout,
        }
    }

    /// `FFmpeg` が利用可能か
    pub(crate) fn is_available(&self) -> bool {
        self.ffmpeg_path.is_some()
    }

    /// 拡張子が remux 対象か判定する
    pub(crate) fn needs_remux(ext: &str) -> bool {
        REMUX_EXTENSIONS.contains(&ext)
    }

    /// 動画から 1 フレームを JPEG バイト列として抽出する
    ///
    /// `ffmpeg -ss {seek} -i {source} -vframes 1 -f image2pipe -vcodec mjpeg pipe:1`
    /// 入力パスは `PathSecurity::validate()` 検証済みであること。
    /// 失敗・タイムアウト時は `None` を返す。
    pub(crate) fn extract_frame(&self, source: &Path) -> Option<Vec<u8>> {
        let ffmpeg = self.ffmpeg_path.as_ref()?;

        let output = std::process::Command::new(ffmpeg)
            .args([
                "-ss",
                &self.thumb_seek_seconds.to_string(),
                "-i",
                &source.to_string_lossy(),
                "-vframes",
                "1",
                "-f",
                "image2pipe",
                "-vcodec",
                "mjpeg",
                "pipe:1",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        match output {
            Ok(out) if out.status.success() && !out.stdout.is_empty() => Some(out.stdout),
            _ => None,
        }
    }

    /// MKV → MP4 remux を実行し、キャッシュされたパスを返す
    ///
    /// `ffmpeg -y -i {source} -c copy -movflags +faststart {dest}`
    /// 入力パスは `PathSecurity::validate()` 検証済みであること。
    /// キャッシュヒット時はキャッシュパスを返す。
    pub(crate) fn get_remuxed(&self, source: &Path, mtime_ns: u128) -> Option<PathBuf> {
        let ffmpeg = self.ffmpeg_path.as_ref()?;

        let cache_key = make_remux_cache_key(source, mtime_ns);

        // キャッシュヒット
        if let Some(cached) = self.cache.get(&cache_key) {
            return Some(cached);
        }

        let ffmpeg = ffmpeg.clone();
        let source_str = source.to_string_lossy().to_string();

        // put_with_writer で一時ファイルに書き込み → キャッシュ登録
        self.cache
            .put_with_writer(
                &cache_key,
                |dest_path| {
                    let output = std::process::Command::new(&ffmpeg)
                        .args([
                            "-y",
                            "-i",
                            &source_str,
                            "-c",
                            "copy",
                            "-movflags",
                            "+faststart",
                            &dest_path.to_string_lossy(),
                        ])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .output()
                        .map_err(|e| {
                            std::io::Error::other(format!("ffmpeg remux 実行失敗: {e}"))
                        })?;

                    if !output.status.success() {
                        return Err(std::io::Error::other("ffmpeg remux 失敗"));
                    }

                    // ffmpeg は dest_path に直接書き込むため、
                    // put_with_writer のパスに既にデータがある
                    // ただし put_with_writer は persist を呼ぶので問題ない
                    Ok(())
                },
                ".mp4",
            )
            .ok()
    }
}

/// `ffmpeg` コマンドのパスを探す
fn find_ffmpeg() -> Option<PathBuf> {
    // `ffmpeg --version` が成功すれば利用可能
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .filter(std::process::ExitStatus::success)
        .map(|_| PathBuf::from("ffmpeg"))
}

/// remux キャッシュキーを生成する: `MD5("{source_path}:{mtime_ns}:remux")`
fn make_remux_cache_key(source: &Path, mtime_ns: u128) -> String {
    use md5::{Digest, Md5};
    let input = format!("{}:{mtime_ns}:remux", source.display());
    let hash = Md5::digest(input.as_bytes());
    format!("{hash:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_remuxがmkvでtrueを返す() {
        assert!(VideoConverter::needs_remux(".mkv"));
    }

    #[test]
    fn needs_remuxがmp4でfalseを返す() {
        assert!(!VideoConverter::needs_remux(".mp4"));
    }

    #[test]
    fn make_remux_cache_keyが決定的な値を返す() {
        let key1 = make_remux_cache_key(Path::new("/mnt/video.mkv"), 1_000_000);
        let key2 = make_remux_cache_key(Path::new("/mnt/video.mkv"), 1_000_000);
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn make_remux_cache_keyがmtime_nsで異なる値を返す() {
        let key1 = make_remux_cache_key(Path::new("/mnt/video.mkv"), 1_000_000);
        let key2 = make_remux_cache_key(Path::new("/mnt/video.mkv"), 2_000_000);
        assert_ne!(key1, key2);
    }
}
