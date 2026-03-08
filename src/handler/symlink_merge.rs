//! シンボリックリンクのマージ実行。
//! Side ベースの統一 API に委譲。

use crate::app::AppState;
use crate::diff::engine::DiffResult;
use crate::merge::executor::MergeDirection;
use crate::runtime::TuiRuntime;

/// シンボリックリンクのマージを実行する。
///
/// ソース側のリンクターゲットをターゲット側に適用する。
/// - LocalToRemote: ローカルのリンク先をリモートに反映
/// - RemoteToLocal: リモートのリンク先をローカルに反映
pub fn execute_symlink_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    direction: MergeDirection,
) {
    let source_target = match &state.current_diff {
        Some(DiffResult::SymlinkDiff {
            left_target,
            right_target,
        }) => match direction {
            MergeDirection::LeftToRight => left_target.clone(),
            MergeDirection::RightToLeft => right_target.clone(),
        },
        _ => {
            state.status_message = "Not a symlink diff".to_string();
            return;
        }
    };

    let source_target = match source_target {
        Some(t) => t,
        None => {
            state.status_message = format!("{}: source symlink target not available", path);
            return;
        }
    };

    let (target_side, src_label, dst_label) = match direction {
        MergeDirection::LeftToRight => (
            &state.right_source,
            state.left_source.display_name(),
            state.right_source.display_name(),
        ),
        MergeDirection::RightToLeft => (
            &state.left_source,
            state.right_source.display_name(),
            state.left_source.display_name(),
        ),
    };

    // リモート側への書き込み時は接続チェック
    if !runtime.is_side_available(target_side) {
        state.status_message = "SSH not connected: cannot merge symlink".to_string();
        return;
    }

    match runtime.create_symlink(target_side, path, &source_target) {
        Ok(()) => {
            state.status_message = format!(
                "{}: symlink {} -> {} merged (-> {})",
                path, src_label, dst_label, source_target
            );
        }
        Err(e) => {
            state.status_message = format!("Symlink merge failed: {}", e);
        }
    }
}
