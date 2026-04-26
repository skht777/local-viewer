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
use crate::services::thumbnail_inflight::{Acquired, InflightLocks};

/// サムネイル生成サービス
///
/// - `generate_from_path`: ファイルパスから生成 (画像ファイル用)
/// - `generate_from_bytes`: バイト列から生成 (アーカイブエントリ用)
/// - `generate_pdf_thumbnail`: `pdftoppm` subprocess で PDF 先頭ページのサムネイルを生成
/// - `make_cache_key`: Python 版と同一のキャッシュキーを生成
/// - `generate_with_dedup`: archive/video の前段処理を含めて重複生成を抑止する汎用ラッパー
pub(crate) struct ThumbnailService {
    cache: Arc<TempFileCache>,
    inflight: Arc<InflightLocks>,
    default_width: u32,
    jpeg_quality: u8,
}

impl ThumbnailService {
    pub(crate) fn new(cache: Arc<TempFileCache>, inflight: Arc<InflightLocks>) -> Self {
        Self {
            cache,
            inflight,
            default_width: 300,
            jpeg_quality: 80,
        }
    }

    /// 生成タスクを `cache_key` 単位で重複排除する汎用ラッパー
    ///
    /// - cache hit ならそのまま返す
    /// - Owner になれたら generator を呼び実生成
    /// - 他者が生成中なら Waiter として待ち、完了後に cache 再読
    /// - Owner 失敗時は最大 1 回再試行（合計 2 回）
    ///
    /// 副作用は generator 内で完結させる（`*_inner` 系は内部で `cache.put` まで行う）。
    pub(crate) fn generate_with_dedup<F>(
        &self,
        cache_key: &str,
        generator: F,
    ) -> Result<Vec<u8>, AppError>
    where
        F: Fn() -> Result<Vec<u8>, AppError>,
    {
        const MAX_ATTEMPTS: usize = 2;
        for attempt in 0..MAX_ATTEMPTS {
            // 1. Cache hit 早期リターン
            if let Some(cached) = self.try_read_cached(cache_key) {
                return Ok(cached);
            }

            // 2. Inflight lock acquire
            match self.inflight.acquire(cache_key) {
                Acquired::Owner(_guard) => {
                    // 3a. Owner: 実生成前にもう一度 cache を確認する
                    // 直前 Owner が cache.put 済みで Drop 中（map remove 完了直後 〜 done 通知前）
                    // に別スレッドが「miss → acquire」したケースを救う（再生成を回避）
                    if let Some(cached) = self.try_read_cached(cache_key) {
                        return Ok(cached);
                    }
                    return generator();
                }
                Acquired::Waiter(handle) => {
                    handle.wait_blocking();
                    if attempt + 1 == MAX_ATTEMPTS {
                        // 上限到達: cache 再読で最終チェック、無ければ Err
                        return self.try_read_cached(cache_key).ok_or_else(|| {
                            AppError::InvalidImage(
                                "サムネイル生成中の他リクエストが失敗しました".to_string(),
                            )
                        });
                    }
                    // continue: ループ先頭で cache hit ならリターン、miss なら再 acquire
                }
            }
        }
        unreachable!()
    }

    /// ファイルパスから JPEG サムネイルを生成する (画像ファイル用)
    ///
    /// `generate_with_dedup` で同一 `cache_key` の重複生成を抑止する。
    pub(crate) fn generate_from_path(
        &self,
        path: &Path,
        cache_key: &str,
    ) -> Result<Vec<u8>, AppError> {
        self.generate_with_dedup(cache_key, || self.generate_from_path_inner(path, cache_key))
    }

    /// `generate_from_path` の実生成本体 (cache.get 早期リターンを行わない)
    ///
    /// ディスクキャッシュからの読み込みを行わず、必ず生成して `cache.put` する。
    /// `generate_with_dedup` のクロージャ内から直接呼ぶ用途で `pub(crate)`。
    pub(crate) fn generate_from_path_inner(
        &self,
        path: &Path,
        cache_key: &str,
    ) -> Result<Vec<u8>, AppError> {
        let source_bytes = std::fs::read(path)
            .map_err(|e| AppError::InvalidImage(format!("ファイル読み込み失敗: {e}")))?;

        let thumb = resize_to_jpeg(&source_bytes, self.default_width, self.jpeg_quality)?;

        let _ = self.cache.put(cache_key, &thumb, ".jpg");
        Ok(thumb)
    }

    /// バイト列から JPEG サムネイルを生成する (アーカイブエントリ用)
    pub(crate) fn generate_from_bytes(
        &self,
        source_bytes: &[u8],
        cache_key: &str,
    ) -> Result<Vec<u8>, AppError> {
        self.generate_with_dedup(cache_key, || {
            self.generate_from_bytes_inner(source_bytes, cache_key)
        })
    }

    /// `generate_from_bytes` の実生成本体 (cache.get 早期リターンを行わない)
    pub(crate) fn generate_from_bytes_inner(
        &self,
        source_bytes: &[u8],
        cache_key: &str,
    ) -> Result<Vec<u8>, AppError> {
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
        self.generate_with_dedup(cache_key, || {
            self.generate_pdf_thumbnail_inner(pdf_path, cache_key, timeout_secs)
        })
    }

    /// `generate_pdf_thumbnail` の実生成本体 (cache.get 早期リターンを行わない)
    pub(crate) fn generate_pdf_thumbnail_inner(
        &self,
        pdf_path: &Path,
        cache_key: &str,
        timeout_secs: u64,
    ) -> Result<Vec<u8>, AppError> {
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

    /// キャッシュからサムネイルバイト列を読み込む (ヒット時のみ)
    ///
    /// アーカイブサムネイル等で重い外部プロセスを呼ぶ前の早期リターンに使用。
    pub(crate) fn try_read_cached(&self, cache_key: &str) -> Option<Vec<u8>> {
        let cached_path = self.cache.get(cache_key)?;
        std::fs::read(&cached_path).ok()
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

/// 子プロセスをタイムアウト付きで待機する（チャネルベース）
///
/// 別スレッドで OS ネイティブの `wait` を実行し、`recv_timeout` でタイムアウト判定。
/// 超過時は `Child::kill()` で安全に終了する。
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    // stderr を先に取り出す（wait 後は take できない）
    let stderr_pipe = child.stderr.take();

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
        let status = result?;
        let stderr = stderr_pipe.map_or_else(Vec::new, |mut s| {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut s, &mut buf).unwrap_or_default();
            buf
        });
        Ok(std::process::Output {
            status,
            stdout: Vec::new(),
            stderr,
        })
    } else {
        let mut c = child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = c.kill();
        let _ = c.wait();
        Err(std::io::Error::other("pdftoppm タイムアウト"))
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
        let inflight = InflightLocks::new();
        (ThumbnailService::new(cache, inflight), dir)
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

    #[test]
    fn 同一cache_keyへの並列generate_with_dedupで正常系のgeneratorは1回しか呼ばれない() {
        use std::sync::Barrier;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;
        use std::time::Duration;

        let (svc, _dir) = make_service();
        let svc = Arc::new(svc);
        let n = 8;
        let barrier = Arc::new(Barrier::new(n));
        let counter = Arc::new(AtomicUsize::new(0));
        let cache_key = "parallel_dedup_key";
        let source = Arc::new(minimal_jpeg());

        let handles: Vec<_> = (0..n)
            .map(|_| {
                let svc = Arc::clone(&svc);
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                let source = Arc::clone(&source);
                thread::spawn(move || {
                    // 全スレッドが同時に generate_with_dedup を呼ぶ
                    barrier.wait();
                    svc.generate_with_dedup(cache_key, || {
                        counter.fetch_add(1, Ordering::Relaxed);
                        // Owner 在籍時間を確保して他スレッドを Waiter 化させる
                        thread::sleep(Duration::from_millis(50));
                        svc.generate_from_bytes_inner(&source, cache_key)
                    })
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // 全件 Ok で結果バイト列が一致
        let first = results[0].as_ref().unwrap().clone();
        for r in &results {
            let bytes = r.as_ref().unwrap();
            assert_eq!(bytes, &first);
        }
        // 正常系では generator は 1 回のみ
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn owner_generatorがerrを返した場合waiterが再試行する() {
        use std::sync::Barrier;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;
        use std::time::Duration;

        let (svc, _dir) = make_service();
        let svc = Arc::new(svc);
        let n = 4;
        let barrier = Arc::new(Barrier::new(n));
        let counter = Arc::new(AtomicUsize::new(0));
        let cache_key = "retry_key";
        let source = Arc::new(minimal_jpeg());

        let handles: Vec<_> = (0..n)
            .map(|_| {
                let svc = Arc::clone(&svc);
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                let source = Arc::clone(&source);
                thread::spawn(move || {
                    barrier.wait();
                    svc.generate_with_dedup(cache_key, || {
                        let n = counter.fetch_add(1, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(30));
                        if n == 0 {
                            // 1 回目は Err を返す（Owner 失敗）
                            Err(AppError::InvalidImage("test failure".to_string()))
                        } else {
                            // 2 回目以降は Ok（再試行で成功）
                            svc.generate_from_bytes_inner(&source, cache_key)
                        }
                    })
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // 1 件は Err、残りは Ok（Waiter のリトライで成功）
        let ok_count = results.iter().filter(|r| r.is_ok()).count();
        let err_count = results.iter().filter(|r| r.is_err()).count();
        assert!(ok_count >= 1, "ok_count = {ok_count}");
        assert_eq!(ok_count + err_count, n);
        // generator は最大 2 回（初回 Err + リトライ Ok）
        let calls = counter.load(Ordering::Relaxed);
        assert!(
            (1..=2).contains(&calls),
            "generator calls = {calls} (expected 1..=2)"
        );
    }
}
