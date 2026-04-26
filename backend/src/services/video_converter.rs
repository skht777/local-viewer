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

        let child = std::process::Command::new(ffmpeg)
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
            .spawn()
            .ok()?;

        let timeout = std::time::Duration::from_secs(self.thumb_timeout);
        let output = wait_with_timeout_and_stdout(child, timeout).ok()?;

        if output.status.success() && !output.stdout.is_empty() {
            Some(output.stdout)
        } else {
            None
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
        let source_str = source.to_string_lossy().into_owned();

        // put_with_writer で一時ファイルに書き込み → キャッシュ登録
        let remux_timeout = self.remux_timeout;
        self.cache
            .put_with_writer(
                &cache_key,
                |dest_path| {
                    let child = std::process::Command::new(&ffmpeg)
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
                        .spawn()
                        .map_err(|e| {
                            std::io::Error::other(format!("ffmpeg remux 実行失敗: {e}"))
                        })?;

                    let timeout = std::time::Duration::from_secs(remux_timeout);
                    let output = wait_with_timeout_and_stdout(child, timeout)?;

                    if !output.status.success() {
                        return Err(std::io::Error::other("ffmpeg remux 失敗"));
                    }

                    Ok(())
                },
                ".mp4",
            )
            .ok()
    }
}

/// 子プロセスをタイムアウト付きで待機する
///
/// stdout を先に読み取り、別スレッドで `wait` を実行。
/// チャネルでタイムアウト判定し、超過時は kill する。
/// ポーリングではなく OS ネイティブの wait を使用し、スレッドプール効率を改善する。
fn wait_with_timeout_and_stdout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    // stdout を先に読む (パイプバッファ溢れによるデッドロック防止)
    let stdout = child.stdout.take().map_or_else(Vec::new, |mut s| {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut s, &mut buf).unwrap_or_default();
        buf
    });

    // チャネルベースのタイムアウト待機
    let child = std::sync::Arc::new(std::sync::Mutex::new(child));
    let child_clone = std::sync::Arc::clone(&child);
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = child_clone
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .wait();
        let _ = tx.send(result);
    });

    if let Ok(result) = rx.recv_timeout(timeout) {
        Ok(std::process::Output {
            status: result?,
            stdout,
            stderr: Vec::new(),
        })
    } else {
        let mut c = child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = c.kill();
        let _ = c.wait();
        Err(std::io::Error::other("ffmpeg タイムアウト"))
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
    hex::encode(hash)
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
