//! notify イベントを pending マップに蓄積する分類ロジック

use std::collections::HashMap;
use std::path::PathBuf;

use notify::EventKind;
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};

use super::filter::enqueue;

/// notify イベントを pending マップに蓄積する
///
/// Create / Rename-To を "add"、Remove / Rename-From を "remove" として扱う。
/// 他のイベント種別 (`Modify::Data`, `Access` 等) は index 更新不要のため無視する。
pub(super) fn handle_notify_event(
    pending: &std::sync::Mutex<HashMap<String, String>>,
    event: &notify::Event,
    mounts: &[(String, PathBuf)],
) {
    let action = match &event.kind {
        EventKind::Create(CreateKind::File | CreateKind::Folder)
        | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => "add",
        EventKind::Remove(RemoveKind::File | RemoveKind::Folder)
        | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => "remove",
        _ => return,
    };

    for path in &event.paths {
        enqueue(pending, path, action, mounts);
    }
}
