//! pending への enqueue と隠しフィルタ

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// 対象パスを pending に追加する (隠しファイル/ディレクトリのみスキップ)
///
/// 拡張子では絞らない: `DirIndex` は画像を含む全エントリを保持するため、
/// どのファイルの追加/削除でも親ディレクトリの dirty 化が必要。
/// FTS インデックスへの反映可否は `worker::process_event` の
/// `classify_for_index` が判定する。
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
