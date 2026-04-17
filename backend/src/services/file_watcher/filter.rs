//! pending への enqueue と隠し/拡張子フィルタ

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::services::extensions::{
    ARCHIVE_EXTENSIONS, PDF_EXTENSIONS, VIDEO_EXTENSIONS, extract_extension,
};

/// 対象パスを pending に追加する (隠しファイル・非対象拡張子をスキップ)
pub(super) fn enqueue(
    pending: &std::sync::Mutex<HashMap<String, String>>,
    path: &Path,
    action: &str,
    mounts: &[(String, PathBuf)],
) {
    // 隠しファイル/ディレクトリをスキップ (full scan の parallel_walk と同じ判定基準)
    if is_hidden_under_mounts(path, mounts) {
        return;
    }

    // ファイルの場合: 拡張子チェック (ディレクトリは常に通過)
    // Remove イベントでは path.is_file() が false になるため、
    // ディレクトリ判定には is_dir() ではなく拡張子の有無で判断
    let Some(file_name) = path.file_name() else {
        return;
    };
    let name = file_name.to_string_lossy();
    let ext = extract_extension(&name).to_lowercase();

    // 拡張子がない → ディレクトリとみなして通過
    // 拡張子がある → インデックス対象かチェック
    if !ext.is_empty() && !is_indexable_extension(&ext) {
        return;
    }

    let key = path.to_string_lossy().into_owned();
    let mut guard = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.insert(key, action.to_string());
}

/// パスがマウント配下で隠し要素 (名前が '.' で始まる) を含むか判定する
///
/// - マウントルートからの相対パスを取り、各コンポーネントを検査
/// - いずれかのコンポーネント名が '.' で始まるなら hidden
/// - マウントルート自身の名前は判定対象外（`parallel_walk::scan_one` の BFS 起点が
///   `skip_hidden` の対象外であるのと一致。`/data/.archive` をマウント登録しても配下は走査される）
/// - マウント外パスは fail-safe として hidden 扱い（FileWatcher は通常マウント配下のみ監視）
pub(super) fn is_hidden_under_mounts(path: &Path, mounts: &[(String, PathBuf)]) -> bool {
    for (_, root) in mounts {
        if let Ok(rel) = path.strip_prefix(root) {
            return rel.components().any(|comp| {
                if let Component::Normal(name) = comp {
                    name.to_string_lossy().starts_with('.')
                } else {
                    false
                }
            });
        }
    }
    true
}

/// 拡張子がインデックス対象 (動画/アーカイブ/PDF) か判定する
///
/// 画像はファイル数が膨大になるため除外 (`classify_for_index` と同じ方針)
pub(super) fn is_indexable_extension(ext: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&ext)
        || ARCHIVE_EXTENSIONS.contains(&ext)
        || PDF_EXTENSIONS.contains(&ext)
}
