//! ディスク LRU キャッシュ
//!
//! サムネイル JPEG、MKV remux 出力等をディスクに保持する。
//! - アトミック書き込み (tempfile → rename)
//! - サイズベース LRU eviction
//! - スレッドセーフ (内部 Mutex)

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// ディスク LRU キャッシュ
///
/// - `get`: キャッシュヒット時にパスを返す (LRU 更新)
/// - `put`: バイト列をアトミックに書き込み、キャッシュに登録
/// - `put_with_writer`: コールバックで一時ファイルに書き込み、キャッシュに登録
/// - 合計サイズが `max_size_bytes` を超えると最古エントリを eviction
pub(crate) struct TempFileCache {
    inner: Mutex<CacheInner>,
}

struct CacheInner {
    cache_dir: PathBuf,
    max_size_bytes: u64,
    /// key → (ファイルパス, ファイルサイズ)
    entries: HashMap<String, (PathBuf, u64)>,
    /// LRU 順序 (front=oldest, back=newest)
    order: VecDeque<String>,
    current_bytes: u64,
}

impl TempFileCache {
    /// 新しいキャッシュを作成する
    ///
    /// `cache_dir` が存在しない場合は再帰的に作成する。
    pub(crate) fn new(cache_dir: PathBuf, max_size_bytes: u64) -> std::io::Result<Self> {
        fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            inner: Mutex::new(CacheInner {
                cache_dir,
                max_size_bytes,
                entries: HashMap::new(),
                order: VecDeque::new(),
                current_bytes: 0,
            }),
        })
    }

    /// キャッシュからパスを取得する
    ///
    /// ヒット時は LRU を更新して `Some(path)` を返す。
    /// ファイルが消失している場合はエントリを削除して `None` を返す。
    pub(crate) fn get(&self, key: &str) -> Option<PathBuf> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (path, size) = inner.entries.get(key)?;
        let path = path.clone();
        let size = *size;

        // ファイルが消失している場合はエントリを削除
        if !path.exists() {
            inner.entries.remove(key);
            inner.order.retain(|k| k != key);
            inner.current_bytes = inner.current_bytes.saturating_sub(size);
            return None;
        }

        // LRU 更新: 末尾に移動
        inner.order.retain(|k| k != key);
        inner.order.push_back(key.to_string());

        Some(path)
    }

    /// バイト列をキャッシュに書き込む
    ///
    /// 1. 一時ファイルに書き込み
    /// 2. アトミックに最終パスへ rename
    /// 3. LRU に登録 (必要に応じて eviction)
    pub(crate) fn put(&self, key: &str, data: &[u8], suffix: &str) -> std::io::Result<PathBuf> {
        let cache_dir = {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            inner.cache_dir.clone()
        };

        let final_path = cache_dir.join(format!("{key}{suffix}"));
        let size = data.len() as u64;

        // ロック外でファイル I/O を実行
        atomic_write(&cache_dir, &final_path, data)?;

        // ロック取得して LRU 登録
        self.register_entry(key, final_path.clone(), size);

        Ok(final_path)
    }

    /// コールバックで一時ファイルに書き込み、キャッシュに登録する
    ///
    /// `FFmpeg` remux 等、出力サイズが事前に不明な場合に使用する。
    /// `writer` には一時ファイルのパスが渡される。
    pub(crate) fn put_with_writer<F>(
        &self,
        key: &str,
        writer: F,
        suffix: &str,
    ) -> std::io::Result<PathBuf>
    where
        F: FnOnce(&Path) -> std::io::Result<()>,
    {
        let cache_dir = {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            inner.cache_dir.clone()
        };

        let final_path = cache_dir.join(format!("{key}{suffix}"));

        // 一時ファイルを作成してコールバックで書き込み
        let tmp = tempfile::NamedTempFile::new_in(&cache_dir)?;
        let tmp_path = tmp.path().to_path_buf();

        writer(&tmp_path)?;

        let actual_size = fs::metadata(&tmp_path)?.len();

        // アトミックに最終パスへ移動
        // persist_noclobber ではなく persist で上書き許可
        tmp.persist(&final_path).map_err(std::io::Error::other)?;

        // LRU に登録
        self.register_entry(key, final_path.clone(), actual_size);

        Ok(final_path)
    }

    /// エントリを LRU に登録し、必要に応じて eviction を実行する
    fn register_entry(&self, key: &str, path: PathBuf, size: u64) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // 既存エントリがあれば削除
        if let Some((old_path, old_size)) = inner.entries.remove(key) {
            inner.current_bytes = inner.current_bytes.saturating_sub(old_size);
            inner.order.retain(|k| k != key);
            // 古いファイルが別パスなら削除
            if old_path != path {
                let _ = fs::remove_file(&old_path);
            }
        }

        // eviction: 合計サイズ超過時に最古エントリを削除
        while inner.current_bytes + size > inner.max_size_bytes && !inner.order.is_empty() {
            if let Some(evict_key) = inner.order.pop_front() {
                if let Some((evict_path, evict_size)) = inner.entries.remove(&evict_key) {
                    inner.current_bytes = inner.current_bytes.saturating_sub(evict_size);
                    let _ = fs::remove_file(&evict_path);
                }
            }
        }

        // 新エントリを登録
        inner.entries.insert(key.to_string(), (path, size));
        inner.order.push_back(key.to_string());
        inner.current_bytes += size;
    }
}

/// バイト列をアトミックに書き込む (tempfile → rename)
fn atomic_write(dir: &Path, final_path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.as_file().write_all(data)?;
    tmp.persist(final_path).map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cache(max_bytes: u64) -> (TempFileCache, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let cache = TempFileCache::new(dir.path().to_path_buf(), max_bytes).unwrap();
        (cache, dir)
    }

    #[test]
    fn 空のキャッシュでgetがnoneを返す() {
        let (cache, _dir) = make_cache(1024);
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn putしたデータがgetで取得できる() {
        let (cache, _dir) = make_cache(1024);

        let path = cache.put("key1", b"hello", ".jpg").unwrap();
        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), b"hello");

        let cached = cache.get("key1");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), path);
    }

    #[test]
    fn 同一キーのputが上書きする() {
        let (cache, _dir) = make_cache(1024);

        cache.put("key1", b"old", ".jpg").unwrap();
        let new_path = cache.put("key1", b"new", ".jpg").unwrap();

        assert_eq!(fs::read(&new_path).unwrap(), b"new");

        let inner = cache.inner.lock().unwrap();
        assert_eq!(inner.entries.len(), 1);
        assert_eq!(inner.current_bytes, 3); // "new" = 3 bytes
    }

    #[test]
    fn evictionが最古のエントリを削除する() {
        // 最大 10 バイト
        let (cache, _dir) = make_cache(10);

        let path_a = cache.put("a", b"12345", ".dat").unwrap(); // 5 bytes
        let _path_b = cache.put("b", b"12345", ".dat").unwrap(); // 5 bytes, total=10

        // 合計 10 → "c" (5 bytes) 追加で超過 → "a" が evict
        let _path_c = cache.put("c", b"12345", ".dat").unwrap();

        assert!(cache.get("a").is_none());
        assert!(!path_a.exists());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn put_with_writerでファイル書き込みできる() {
        let (cache, _dir) = make_cache(1024);

        let path = cache
            .put_with_writer(
                "writer_key",
                |tmp_path| fs::write(tmp_path, b"written by callback"),
                ".mp4",
            )
            .unwrap();

        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), b"written by callback");
        assert!(cache.get("writer_key").is_some());
    }

    #[test]
    fn 存在しないファイルのgetがnoneを返しエントリを削除する() {
        let (cache, _dir) = make_cache(1024);

        let path = cache.put("vanish", b"data", ".jpg").unwrap();

        // ファイルを手動削除
        fs::remove_file(&path).unwrap();

        assert!(cache.get("vanish").is_none());

        // エントリも削除されていること
        let inner = cache.inner.lock().unwrap();
        assert!(!inner.entries.contains_key("vanish"));
        assert_eq!(inner.current_bytes, 0);
    }

    #[test]
    fn getでlruが更新される() {
        // 最大 15 バイト
        let (cache, _dir) = make_cache(15);

        cache.put("a", b"12345", ".dat").unwrap(); // 5 bytes
        cache.put("b", b"12345", ".dat").unwrap(); // 5 bytes
        cache.put("c", b"12345", ".dat").unwrap(); // 5 bytes, total=15

        // "a" を get して LRU 更新 → "a" が最新に
        assert!(cache.get("a").is_some());

        // "d" 追加 → "b" が evict される (最古)
        cache.put("d", b"12345", ".dat").unwrap();

        assert!(cache.get("a").is_some()); // LRU 更新済みなので残る
        assert!(cache.get("b").is_none()); // evict された
        assert!(cache.get("c").is_some());
        assert!(cache.get("d").is_some());
    }
}
