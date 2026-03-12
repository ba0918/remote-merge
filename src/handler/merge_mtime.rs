//! mtime 衝突チェック（楽観的ロック）。
//!
//! マージ実行前にファイルの mtime が diff 取得時から変更されていないかチェックし、
//! 衝突があれば MtimeWarningDialog を表示する。

use crate::app::AppState;
use crate::merge::executor::MergeDirection;
use crate::merge::optimistic_lock::{self, MtimeConflict};
use crate::runtime::TuiRuntime;
use crate::ui::dialog::{DialogState, MtimeWarningDialog, MtimeWarningMergeContext};

use super::merge_file_io::stat_left_file;

/// 単一ファイルの楽観的ロックチェック。
///
/// マージ先のファイルの mtime が diff 取得時から変更されていないかチェック。
/// 衝突がある場合は MtimeWarningDialog を表示し `true` を返す。
pub fn check_mtime_conflict_single(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    direction: MergeDirection,
) -> bool {
    match direction {
        MergeDirection::LeftToRight => {
            // リモート側のmtimeをチェック
            if !runtime.is_side_available(&state.right_source) {
                return false;
            }
            let expected = state
                .right_tree
                .find_node(std::path::Path::new(path))
                .and_then(|n| n.mtime);

            match runtime.stat_files(&state.right_source, &[path.to_string()]) {
                Ok(results) => {
                    let actual = results.first().and_then(|(_, dt)| *dt);
                    if let Some(conflict) = optimistic_lock::check_mtime(path, expected, actual) {
                        show_mtime_warning(state, vec![conflict], direction, Some(path));
                        return true;
                    }
                }
                Err(e) => {
                    tracing::warn!("mtime check failed (continuing): {}", e);
                }
            }
        }
        MergeDirection::RightToLeft => {
            // 左側のmtimeをチェック（ローカル or リモート）
            let expected = state
                .left_tree
                .find_node(std::path::Path::new(path))
                .and_then(|n| n.mtime);
            let actual = stat_left_file(state, runtime, path);

            if let Some(conflict) = optimistic_lock::check_mtime(path, expected, actual) {
                show_mtime_warning(state, vec![conflict], direction, Some(path));
                return true;
            }
        }
    }
    false
}

fn show_mtime_warning(
    state: &mut AppState,
    conflicts: Vec<MtimeConflict>,
    direction: MergeDirection,
    path: Option<&str>,
) {
    let merge_context = match path {
        Some(p) => MtimeWarningMergeContext::Single {
            path: p.to_string(),
            direction,
        },
        None => MtimeWarningMergeContext::Batch { direction },
    };
    state.dialog = DialogState::MtimeWarning(MtimeWarningDialog {
        conflicts,
        merge_context,
    });
}

/// diff viewer からの書き込み時の mtime チェック（w キー / HunkMergePreview）。
///
/// - `hunk_direction` が `Some` なら HunkMerge コンテキスト
/// - `None` なら Write コンテキスト（w キーで両側書き込み）
///
/// 衝突があれば `MtimeWarningDialog` を表示して `true` を返す。
pub fn check_mtime_for_write(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    hunk_direction: Option<crate::diff::engine::HunkDirection>,
) -> bool {
    let path = match &state.selected_path {
        Some(p) => p.clone(),
        None => return false,
    };

    // 左側・右側の mtime を取得
    let left_expected = state
        .left_tree
        .find_node(std::path::Path::new(&path))
        .and_then(|n| n.mtime);
    let left_actual = stat_left_file(state, runtime, &path);

    let has_right = runtime.is_side_available(&state.right_source);
    let (right_expected, right_actual) = if has_right {
        let expected = state
            .right_tree
            .find_node(std::path::Path::new(&path))
            .and_then(|n| n.mtime);
        let actual = match runtime.stat_files(&state.right_source, std::slice::from_ref(&path)) {
            Ok(results) => results.first().and_then(|(_, dt)| *dt),
            Err(e) => {
                tracing::warn!("mtime check failed (continuing): {}", e);
                None
            }
        };
        (expected, actual)
    } else {
        (None, None)
    };

    // 純粋関数で衝突判定
    let conflicts = super::merge_mtime_logic::collect_write_conflicts(
        &path,
        left_expected,
        left_actual,
        right_expected,
        right_actual,
        has_right,
    );

    if conflicts.is_empty() {
        return false;
    }

    let merge_context = match hunk_direction {
        Some(dir) => MtimeWarningMergeContext::HunkMerge { direction: dir },
        None => MtimeWarningMergeContext::Write,
    };
    state.dialog = DialogState::MtimeWarning(MtimeWarningDialog {
        conflicts,
        merge_context,
    });
    true
}
