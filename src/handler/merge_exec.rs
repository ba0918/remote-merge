//! マージ実行（単一ファイル・ハンクマージ・書き込み）。
//!
//! バッチマージ → `merge_batch`、mtime チェック → `merge_mtime`、
//! コンテンツロード → `merge_content`、I/Oヘルパー → `merge_file_io` に分離済み。
//! このモジュールは単一ファイルマージ・ハンクマージ・write の実行のみを担当する。

use crate::app::AppState;
use crate::diff::engine::HunkDirection;
use crate::merge::executor::MergeDirection;
use crate::runtime::TuiRuntime;
use crate::ui::dialog::ConfirmDialog;

use super::merge_file_io::{backup_left, backup_right, write_left_file, write_right_file};

// ── 後方互換の re-export ──
pub use super::merge_batch::{execute_batch_merge, filter_unchecked_equal};
pub use super::merge_content::{load_file_content, load_subtree_contents};
pub use super::merge_mtime::{check_mtime_conflict_single, check_mtime_for_write};
pub use super::merge_tree_load::{expand_subtree_for_merge, load_remote_children_to};

/// マージを実行する
pub fn execute_merge(state: &mut AppState, runtime: &mut TuiRuntime, confirm: &ConfirmDialog) {
    use crate::diff::engine::DiffResult;

    let path = &confirm.file_path;
    let direction = confirm.direction;

    // シンボリックリンクの場合は専用のマージ処理
    if let Some(DiffResult::SymlinkDiff { .. }) = &state.current_diff {
        super::symlink_merge::execute_symlink_merge(state, runtime, path, direction);
        return;
    }

    // バイナリファイルのマージは未対応（バイト列I/Oが必要）
    if let Some(DiffResult::Binary { .. }) = &state.current_diff {
        state.status_message = format!("{}: binary file merge is not yet supported", path);
        return;
    }

    match direction {
        MergeDirection::LeftToRight => {
            let content = match state.left_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    state.status_message = format!("{}: left content not loaded", path);
                    return;
                }
            };

            if !state.is_connected {
                state.status_message = "SSH not connected: cannot merge".to_string();
                return;
            }

            if runtime.core.config.backup.enabled {
                backup_right(state, runtime, &[path.to_string()]);
            }

            match write_right_file(state, runtime, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    let left = state.left_source.display_name();
                    let right = state.right_source.display_name();
                    state.status_message = format!("{}: {} -> {} merged", path, left, right);
                }
                Err(e) => {
                    state.status_message = format!("Merge failed: {}", e);
                }
            }
        }
        MergeDirection::RightToLeft => {
            let content = match state.right_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    state.status_message = format!("{}: right content not loaded", path);
                    return;
                }
            };

            if runtime.core.config.backup.enabled {
                backup_left(state, runtime, &[path.to_string()]);
            }

            match write_left_file(state, runtime, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    let left = state.left_source.display_name();
                    let right = state.right_source.display_name();
                    state.status_message = format!("{}: {} -> {} merged", path, right, left);
                }
                Err(e) => {
                    state.status_message = format!("Merge failed: {}", e);
                }
            }
        }
    }
}

/// ハンクマージを実行する（2段階操作の確定時）
pub fn execute_hunk_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    direction: HunkDirection,
) {
    if let Some(path) = state.apply_hunk_merge(direction) {
        if runtime.core.config.backup.enabled {
            match direction {
                HunkDirection::RightToLeft => {
                    backup_left(state, runtime, std::slice::from_ref(&path));
                }
                HunkDirection::LeftToRight => {
                    if state.is_connected {
                        backup_right(state, runtime, std::slice::from_ref(&path));
                    }
                }
            }
        }

        match direction {
            HunkDirection::RightToLeft => {
                let content = state.left_cache.get(&path).cloned().unwrap_or_default();
                match write_left_file(state, runtime, &path, &content) {
                    Ok(()) => {
                        let left = state.left_source.display_name();
                        let right = state.right_source.display_name();
                        state.status_message = format!(
                            "Hunk merged: {} -> {} ({}) | {} hunks left",
                            right,
                            left,
                            path,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        state.status_message = format!("Left write failed: {}", e);
                    }
                }
            }
            HunkDirection::LeftToRight => {
                let content = state.right_cache.get(&path).cloned().unwrap_or_default();
                match write_right_file(state, runtime, &path, &content) {
                    Ok(()) => {
                        let left = state.left_source.display_name();
                        let right = state.right_source.display_name();
                        state.status_message = format!(
                            "Hunk merged: {} -> {} ({}) | {} hunks left",
                            left,
                            right,
                            path,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        state.status_message = format!("Right write failed: {}", e);
                    }
                }
            }
        }
    }
}

/// 変更をファイルに書き込む（w キー確定後）
pub fn execute_write_changes(state: &mut AppState, runtime: &mut TuiRuntime) {
    if let Some(path) = state.selected_path.clone() {
        let changes = state.undo_stack.len();

        if runtime.core.config.backup.enabled {
            backup_left(state, runtime, std::slice::from_ref(&path));
            if state.is_connected {
                backup_right(state, runtime, std::slice::from_ref(&path));
            }
        }

        if let Some(left_content) = state.left_cache.get(&path).cloned() {
            if let Err(e) = write_left_file(state, runtime, &path, &left_content) {
                state.status_message = format!("Left write failed: {}", e);
                return;
            }
        }

        if state.is_connected {
            if let Some(right_content) = state.right_cache.get(&path).cloned() {
                if let Err(e) = write_right_file(state, runtime, &path, &right_content) {
                    state.status_message = format!("Right write failed: {}", e);
                    return;
                }
            }
        }

        state.undo_stack.clear();
        state.status_message = format!(
            "{}: {} changes written | {} hunks remaining",
            path,
            changes,
            state.hunk_count()
        );
    }
}
