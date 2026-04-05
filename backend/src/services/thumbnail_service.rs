//! サムネイル生成サービス
//!
//! - `image` + `fast_image_resize` で 300px JPEG リサイズ
//! - PDF は `pdftoppm` (poppler-utils) subprocess で先頭ページをラスタライズ
//! - `TempFileCache` でディスクキャッシュ
//! - CPU バウンド処理のため `spawn_blocking` 内から呼ぶこと

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageEncoder, RgbImage};

use crate::errors::AppError;
use crate::services::temp_file_cache::TempFileCache;

/// サムネイル生成サービス
///
/// - `generate_from_path`: ファイルパスから生成 (画像ファイル用)
/// - `generate_from_bytes`: バイト列から生成 (アーカイブエントリ用)
/// - `generate_pdf_thumbnail`: `pdftoppm` subprocess で PDF 先頭ページのサムネイルを生成
/// - `make_cache_key`: Python 版と同一のキャッシュキーを生成
pub(crate) struct ThumbnailService {
    cache: Arc<TempFileCache>,
    default_width: u32,
    jpeg_quality: u8,
}

impl ThumbnailService {
    pub(crate) fn new(cache: Arc<TempFileCache>) -> Self {
        Self {
            cache,
            default_width: 300,
            jpeg_quality: 80,
        }
    }

    /// ファイルパスから JPEG サムネイルを生成する (画像ファイル用)
    ///
    /// キャッシュヒット時はキャッシュから読み込む。
    /// ミス時はデコード → リサイズ → JPEG エンコード → キャッシュに書き込み。
    pub(crate) fn generate_from_path(
        &self,
        path: &Path,
        cache_key: &str,
    ) -> Result<Vec<u8>, AppError> {
        if let Some(cached_path) = self.cache.get(cache_key) {
            return std::fs::read(&cached_path)
                .map_err(|e| AppError::InvalidImage(format!("キャッシュ読み込み失敗: {e}")));
        }

        let source_bytes = std::fs::read(path)
            .map_err(|e| AppError::InvalidImage(format!("ファイル読み込み失敗: {e}")))?;

        let thumb = resize_to_jpeg(&source_bytes, self.default_width, self.jpeg_quality)?;

        let _ = self.cache.put(cache_key, &thumb, ".jpg");
        Ok(thumb)
    }

    /// バイト列から JPEG サムネイルを生成する (アーカイブエントリ用)
    ///
    /// キャッシュヒット時はキャッシュから読み込む。
    pub(crate) fn generate_from_bytes(
        &self,
        source_bytes: &[u8],
        cache_key: &str,
    ) -> Result<Vec<u8>, AppError> {
        if let Some(cached_path) = self.cache.get(cache_key) {
            return std::fs::read(&cached_path)
                .map_err(|e| AppError::InvalidImage(format!("キャッシュ読み込み失敗: {e}")));
        }

        let thumb = resize_to_jpeg(source_bytes, self.default_width, self.jpeg_quality)?;

        let _ = self.cache.put(cache_key, &thumb, ".jpg");
        Ok(thumb)
    }

    /// `pdftoppm` subprocess で PDF 先頭ページの JPEG サムネイルを生成する
    ///
    /// 入力パスは `PathSecurity::validate()` 検証済みであること。
    /// 出力先は `tempfile::TempDir` 内で安全に管理する。
    pub(crate) fn generate_pdf_thumbnail(
        &self,
        pdf_path: &Path,
        cache_key: &str,
        timeout_secs: u64,
    ) -> Result<Vec<u8>, AppError> {
        if let Some(cached_path) = self.cache.get(cache_key) {
            return std::fs::read(&cached_path)
                .map_err(|e| AppError::InvalidImage(format!("キャッシュ読み込み失敗: {e}")));
        }

        // 一時ディレクトリを作成 (Drop で自動削除)
        let tmp_dir = tempfile::TempDir::new()
            .map_err(|e| AppError::InvalidImage(format!("一時ディレクトリ作成失敗: {e}")))?;
        let output_prefix = tmp_dir.path().join("page");

        // pdftoppm -jpeg -singlefile -r 150 input.pdf {output_prefix}
        let output = std::process::Command::new("pdftoppm")
            .args([
                "-jpeg",
                "-singlefile",
                "-r",
                "150",
                &pdf_path.to_string_lossy(),
                &output_prefix.to_string_lossy(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|child| {
                wait_with_timeout(child, std::time::Duration::from_secs(timeout_secs))
            })
            .map_err(|e| AppError::InvalidImage(format!("pdftoppm 実行失敗: {e}")))?;

        if !output.status.success() {
            return Err(AppError::InvalidImage(
                "PDF サムネイル生成に失敗しました".to_string(),
            ));
        }

        // pdftoppm は {output_prefix}.jpg を出力する
        let jpg_path = tmp_dir.path().join("page.jpg");
        let raw_bytes = std::fs::read(&jpg_path)
            .map_err(|e| AppError::InvalidImage(format!("PDF サムネイル読み込み失敗: {e}")))?;

        // リサイズして JPEG エンコード
        let thumb = resize_to_jpeg(&raw_bytes, self.default_width, self.jpeg_quality)?;

        let _ = self.cache.put(cache_key, &thumb, ".jpg");
        Ok(thumb)
    }

    /// キャッシュキーを生成する (Python 版と同一フォーマット)
    ///
    /// `MD5("thumb:{mtime_ns}:{node_id}:{width}")`
    pub(crate) fn make_cache_key(&self, node_id: &str, mtime_ns: u128) -> String {
        use md5::{Digest, Md5};
        let input = format!("thumb:{mtime_ns}:{node_id}:{}", self.default_width);
        let hash = Md5::digest(input.as_bytes());
        format!("{hash:x}")
    }

    /// キャッシュにエントリが存在するか確認する (warmer 用)
    pub(crate) fn is_cached(&self, cache_key: &str) -> bool {
        self.cache.get(cache_key).is_some()
    }
}

/// 画像バイト列をデコード → リサイズ → JPEG エンコードする
///
/// - アルファチャネルがある場合は白背景で合成
/// - `max_dim` 以下ならリサイズせず JPEG エンコードのみ
fn resize_to_jpeg(source: &[u8], max_dim: u32, quality: u8) -> Result<Vec<u8>, AppError> {
    let img = image::load_from_memory(source)
        .map_err(|e| AppError::InvalidImage(format!("画像デコード失敗: {e}")))?;

    let (orig_w, orig_h) = img.dimensions();

    // アルファチャネルがある場合は白背景で合成
    let rgb = flatten_alpha(img);

    // 元画像が max_dim 以下ならリサイズ不要
    if orig_w <= max_dim && orig_h <= max_dim {
        return encode_jpeg(&rgb, quality);
    }

    // アスペクト比を保持して外接箱フィット
    let ratio = f64::from(max_dim) / f64::from(orig_w.max(orig_h));
    let new_w = std::cmp::max(1, (f64::from(orig_w) * ratio) as u32);
    let new_h = std::cmp::max(1, (f64::from(orig_h) * ratio) as u32);

    // fast_image_resize でリサイズ
    let src_image =
        fr::images::Image::from_vec_u8(orig_w, orig_h, rgb.into_raw(), fr::PixelType::U8x3)
            .map_err(|e| AppError::InvalidImage(format!("画像変換失敗: {e}")))?;

    let mut dst_image = fr::images::Image::new(new_w, new_h, fr::PixelType::U8x3);

    let mut resizer = fr::Resizer::new();
    resizer
        .resize(
            &src_image,
            &mut dst_image,
            Some(
                &fr::ResizeOptions::new()
                    .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3)),
            ),
        )
        .map_err(|e| AppError::InvalidImage(format!("リサイズ失敗: {e}")))?;

    // JPEG エンコード
    let resized_rgb = RgbImage::from_raw(new_w, new_h, dst_image.into_vec()).ok_or_else(|| {
        AppError::InvalidImage("リサイズ後のバッファ変換に失敗しました".to_string())
    })?;

    encode_jpeg(&resized_rgb, quality)
}

/// RGBA 画像を白背景で合成して RGB に変換する
fn flatten_alpha(img: DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.into_rgb8();
    }

    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = RgbImage::new(width, height);

    for (px, py, pixel) in rgba.enumerate_pixels() {
        let [red, green, blue, alpha_byte] = pixel.0;
        let alpha = f32::from(alpha_byte) / 255.0;
        let bg = 255.0; // 白背景
        let out_r = (f32::from(red) * alpha + bg * (1.0 - alpha)) as u8;
        let out_g = (f32::from(green) * alpha + bg * (1.0 - alpha)) as u8;
        let out_b = (f32::from(blue) * alpha + bg * (1.0 - alpha)) as u8;
        rgb.put_pixel(px, py, image::Rgb([out_r, out_g, out_b]));
    }

    rgb
}

/// RGB 画像を JPEG にエンコードする
fn encode_jpeg(rgb: &RgbImage, quality: u8) -> Result<Vec<u8>, AppError> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    let (width, height) = rgb.dimensions();
    encoder
        .write_image(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| AppError::InvalidImage(format!("JPEG エンコード失敗: {e}")))?;
    Ok(buf.into_inner())
}

/// 子プロセスをタイムアウト付きで待機する
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let stderr = child.stderr.map_or_else(Vec::new, |mut s| {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut s, &mut buf).unwrap_or_default();
                buf
            });
            return Ok(std::process::Output {
                status,
                stdout: Vec::new(),
                stderr,
            });
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("pdftoppm タイムアウト"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

// fast_image_resize の短縮エイリアス
use fast_image_resize as fr;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> (ThumbnailService, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let cache =
            Arc::new(TempFileCache::new(dir.path().to_path_buf(), 10 * 1024 * 1024).unwrap());
        (ThumbnailService::new(cache), dir)
    }

    /// 1x1 赤ピクセルの最小 JPEG を生成する
    fn minimal_jpeg() -> Vec<u8> {
        let img = RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]));
        let mut buf = Cursor::new(Vec::new());
        let encoder = JpegEncoder::new_with_quality(&mut buf, 80);
        encoder
            .write_image(img.as_raw(), 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf.into_inner()
    }

    /// 400x300 の JPEG を生成する (リサイズ対象)
    fn large_jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, image::Rgb([0, 128, 255]));
        let mut buf = Cursor::new(Vec::new());
        let encoder = JpegEncoder::new_with_quality(&mut buf, 80);
        encoder
            .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf.into_inner()
    }

    /// アルファチャネル付き PNG を生成する
    fn png_with_alpha() -> Vec<u8> {
        use image::RgbaImage;
        let img = RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 128]));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn jpegバイト列からサムネイルを生成できる() {
        let (svc, _dir) = make_service();
        let source = minimal_jpeg();
        let result = svc.generate_from_bytes(&source, "test_key").unwrap();
        // JPEG マジックナンバー (SOI マーカー)
        assert_eq!(&result[..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn pngバイト列からサムネイルを生成できる() {
        let (svc, _dir) = make_service();
        let source = png_with_alpha();
        let result = svc.generate_from_bytes(&source, "png_key").unwrap();
        assert_eq!(&result[..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn 横長画像が幅300pxにリサイズされる() {
        let source = large_jpeg(600, 300);
        let thumb = resize_to_jpeg(&source, 300, 80).unwrap();

        // デコードしてサイズを確認
        let img = image::load_from_memory(&thumb).unwrap();
        assert_eq!(img.width(), 300);
        assert_eq!(img.height(), 150);
    }

    #[test]
    fn 縦長画像が高さ300pxにリサイズされる() {
        let source = large_jpeg(200, 600);
        let thumb = resize_to_jpeg(&source, 300, 80).unwrap();

        let img = image::load_from_memory(&thumb).unwrap();
        assert_eq!(img.width(), 100);
        assert_eq!(img.height(), 300);
    }

    #[test]
    fn 小さい画像はリサイズされない() {
        let source = large_jpeg(100, 50);
        let thumb = resize_to_jpeg(&source, 300, 80).unwrap();

        let img = image::load_from_memory(&thumb).unwrap();
        assert_eq!(img.width(), 100);
        assert_eq!(img.height(), 50);
    }

    #[test]
    fn アルファチャンネル付きpngで白背景合成される() {
        let source = png_with_alpha();
        let thumb = resize_to_jpeg(&source, 300, 80).unwrap();

        let img = image::load_from_memory(&thumb).unwrap();
        let pixel = img.get_pixel(50, 50);
        // 赤半透明 + 白背景 → ピンク系
        assert!(pixel[0] > 200); // R は高い
        assert!(pixel[1] > 100); // G は中間
        assert!(pixel[2] > 100); // B は中間
    }

    #[test]
    fn 不正なバイト列でエラーを返す() {
        let (svc, _dir) = make_service();
        let result = svc.generate_from_bytes(b"not an image", "bad_key");
        assert!(result.is_err());
    }

    #[test]
    fn キャッシュヒットで生成がスキップされる() {
        let (svc, _dir) = make_service();
        let source = minimal_jpeg();

        // 1回目: キャッシュミス → 生成
        let result1 = svc.generate_from_bytes(&source, "cache_test").unwrap();
        // 2回目: キャッシュヒット
        let result2 = svc.generate_from_bytes(&source, "cache_test").unwrap();
        assert_eq!(result1, result2);
    }

    #[test]
    fn make_cache_keyが決定的な値を返す() {
        let (svc, _dir) = make_service();
        let key1 = svc.make_cache_key("node123", 1_000_000_000);
        let key2 = svc.make_cache_key("node123", 1_000_000_000);
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32); // MD5 hex = 32 文字
    }

    #[test]
    fn is_cachedがキャッシュ存在を正しく判定する() {
        let (svc, _dir) = make_service();
        assert!(!svc.is_cached("missing_key"));

        let source = minimal_jpeg();
        svc.generate_from_bytes(&source, "cached_key").unwrap();
        assert!(svc.is_cached("cached_key"));
    }
}
