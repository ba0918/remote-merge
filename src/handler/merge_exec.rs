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

use super::merge_file_io::{
    decide_backup_write, reserve_backup_session, save_backups, write_left_file, write_right_file,
    BackupDecision,
};

fn backup_allows_write(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    backups: &[(&crate::app::side::Side, &[String])],
) -> Option<Option<String>> {
    let enabled = runtime.core.config.backup.enabled;
    let session_id = match reserve_backup_session(runtime) {
        Ok(session_id) => session_id,
        Err(error) => {
            state.status_message = format!("Backup failed: {error}");
            return None;
        }
    };
    let results = backups
        .iter()
        .map(|(side, paths)| save_backups(runtime, side, paths, session_id.as_deref()))
        .collect::<Vec<_>>();
    match decide_backup_write(enabled, &results) {
        BackupDecision::Write => Some(session_id),
        BackupDecision::Refuse(message) => {
            state.status_message = message;
            None
        }
    }
}

// ── 後方互換の re-export ──
pub use super::merge_batch::{execute_batch_merge, filter_identical_files};
pub use super::merge_content::{load_file_content, load_subtree_contents};
pub use super::merge_mtime::{check_mtime_conflict_single, check_mtime_for_write};
pub use super::merge_tree_load::{expand_subtree_for_merge, load_children_to, load_ref_children};

/// マージを実行する
pub fn execute_merge(state: &mut AppState, runtime: &mut TuiRuntime, confirm: &ConfirmDialog) {
    use super::merge_exec_logic::{determine_merge_execution, MergeExecutionPlan};
    use crate::service::merge::determine_merge_action;

    let path = &confirm.file_path;
    let direction = confirm.direction;

    // ソース/ターゲットのツリーから MergeAction を決定
    let action = {
        let (source_tree, target_tree) = match direction {
            MergeDirection::LeftToRight => (&state.left_tree, &state.right_tree),
            MergeDirection::RightToLeft => (&state.right_tree, &state.left_tree),
        };
        determine_merge_action(source_tree, target_tree, path)
    };

    // 純粋関数で前判定
    let has_source_cache = match direction {
        MergeDirection::LeftToRight => state.left_cache.contains_key(path),
        MergeDirection::RightToLeft => state.right_cache.contains_key(path),
    };
    let plan = determine_merge_execution(
        &action,
        state.current_diff.as_ref(),
        has_source_cache,
        direction,
    );

    match plan {
        MergeExecutionPlan::SkipDifferentKind => {
            state.status_message =
                format!("{path}: source and destination have different file types");
        }
        MergeExecutionPlan::SymlinkMerge => {
            let (source_side, target_side) = match direction {
                MergeDirection::LeftToRight => {
                    (state.left_source.clone(), state.right_source.clone())
                }
                MergeDirection::RightToLeft => {
                    (state.right_source.clone(), state.left_source.clone())
                }
            };
            let symlink_session_id = match reserve_backup_session(runtime) {
                Ok(session_id) => session_id,
                Err(error) => {
                    state.status_message = format!("Backup failed: {error}");
                    return;
                }
            };
            // 戻り値 (成功=true) をログに記録。state.status_message は execute_symlink_merge 内で設定済み。
            let params = super::symlink_merge::SymlinkMergeParams {
                path,
                direction,
                action,
                source_side: &source_side,
                target_side: &target_side,
                session_id: symlink_session_id.as_deref(),
            };
            let ok = super::symlink_merge::execute_symlink_merge(state, runtime, &params);
            if !ok {
                tracing::warn!("symlink merge failed: {}", path);
            }
            return;
        }
        MergeExecutionPlan::BinaryReject => {
            state.status_message = format!("{}: binary file merge is not yet supported", path);
            return;
        }
        MergeExecutionPlan::CacheMissing { side } => {
            state.status_message = format!("{}: {} content not loaded", path, side);
            return;
        }
        MergeExecutionPlan::TextMerge => {
            // 通常のテキストマージフローへ
        }
    }

    match direction {
        MergeDirection::LeftToRight => {
            // determine_merge_execution で has_source_cache=true を確認済みだが、
            // panic 防止のため defensive に処理する
            let content = match state.left_cache.get(path).cloned() {
                Some(c) => c,
                None => {
                    state.status_message =
                        format!("{}: left content not loaded, cannot merge", path);
                    return;
                }
            };

            if !runtime.is_side_available(&state.right_source) {
                state.status_message = "Right side not available: cannot merge".to_string();
                return;
            }

            let target = state.right_source.clone();
            if backup_allows_write(state, runtime, &[(&target, std::slice::from_ref(path))])
                .is_none()
            {
                return;
            }

            match write_right_file(state, runtime, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    let left = state.left_source.display_name();
                    let right = state.right_source.display_name();
                    tracing::info!("Merge ok: {} ({} -> {})", path, left, right);
                    state.status_message = format_merge_success(path, left, right, direction);
                }
                Err(e) => {
                    tracing::error!("Merge failed: path={}, error={}", path, e);
                    state.status_message = format!("Merge failed: {}", e);
                }
            }
        }
        MergeDirection::RightToLeft => {
            // determine_merge_execution で has_source_cache=true を確認済みだが、
            // panic 防止のため defensive に処理する
            let content = match state.right_cache.get(path).cloned() {
                Some(c) => c,
                None => {
                    state.status_message =
                        format!("{}: right content not loaded, cannot merge", path);
                    return;
                }
            };

            let target = state.left_source.clone();
            if backup_allows_write(state, runtime, &[(&target, std::slice::from_ref(path))])
                .is_none()
            {
                return;
            }

            match write_left_file(state, runtime, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    let left = state.left_source.display_name();
                    let right = state.right_source.display_name();
                    tracing::info!("Merge ok: {} ({} -> {})", path, right, left);
                    state.status_message = format_merge_success(path, left, right, direction);
                }
                Err(e) => {
                    tracing::error!("Merge failed: path={}, error={}", path, e);
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
        let target = match direction {
            HunkDirection::RightToLeft => state.left_source.clone(),
            HunkDirection::LeftToRight => state.right_source.clone(),
        };
        if backup_allows_write(state, runtime, &[(&target, std::slice::from_ref(&path))]).is_none()
        {
            return;
        }

        match direction {
            HunkDirection::RightToLeft => {
                let content = match state.left_cache.get(&path).cloned() {
                    Some(c) => c,
                    None => {
                        state.status_message =
                            format!("{}: left content not loaded, cannot write", path);
                        return;
                    }
                };
                match write_left_file(state, runtime, &path, &content) {
                    Ok(()) => {
                        let left = state.left_source.display_name();
                        let right = state.right_source.display_name();
                        tracing::info!(
                            "Hunk merge ok: {} ({} -> {}), hunks_left={}",
                            path,
                            right,
                            left,
                            state.hunk_count()
                        );
                        state.status_message = format_hunk_merge_success(
                            left,
                            right,
                            &path,
                            direction,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Hunk merge write failed: path={}, side=left, error={}",
                            path,
                            e
                        );
                        state.status_message = format!("Left write failed: {}", e);
                    }
                }
            }
            HunkDirection::LeftToRight => {
                let content = match state.right_cache.get(&path).cloned() {
                    Some(c) => c,
                    None => {
                        state.status_message =
                            format!("{}: right content not loaded, cannot write", path);
                        return;
                    }
                };
                match write_right_file(state, runtime, &path, &content) {
                    Ok(()) => {
                        let left = state.left_source.display_name();
                        let right = state.right_source.display_name();
                        tracing::info!(
                            "Hunk merge ok: {} ({} -> {}), hunks_left={}",
                            path,
                            left,
                            right,
                            state.hunk_count()
                        );
                        state.status_message = format_hunk_merge_success(
                            left,
                            right,
                            &path,
                            direction,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Hunk merge write failed: path={}, side=right, error={}",
                            path,
                            e
                        );
                        state.status_message = format!("Right write failed: {}", e);
                    }
                }
            }
        }
    }
}

/// マージ成功時のステータスメッセージを生成する（純粋関数）。
fn format_merge_success(
    path: &str,
    left_name: &str,
    right_name: &str,
    direction: MergeDirection,
) -> String {
    match direction {
        MergeDirection::LeftToRight => format!("{}: {} -> {} merged", path, left_name, right_name),
        MergeDirection::RightToLeft => format!("{}: {} -> {} merged", path, right_name, left_name),
    }
}

/// ハンクマージ成功時のステータスメッセージを生成する（純粋関数）。
fn format_hunk_merge_success(
    left_name: &str,
    right_name: &str,
    path: &str,
    direction: HunkDirection,
    hunks_left: usize,
) -> String {
    let (src, dst) = match direction {
        HunkDirection::RightToLeft => (right_name, left_name),
        HunkDirection::LeftToRight => (left_name, right_name),
    };
    format!(
        "Hunk merged: {} -> {} ({}) | {} hunks left",
        src, dst, path, hunks_left,
    )
}

/// 変更をファイルに書き込む（w キー確定後）
pub fn execute_write_changes(state: &mut AppState, runtime: &mut TuiRuntime) {
    if let Some(path) = state.selected_path.clone() {
        let changes = state.undo_stack.len();
        let left = state.left_source.clone();
        let right = state.right_source.clone();
        if backup_allows_write(
            state,
            runtime,
            &[
                (&left, std::slice::from_ref(&path)),
                (&right, std::slice::from_ref(&path)),
            ],
        )
        .is_none()
        {
            return;
        }

        if let Some(left_content) = state.left_cache.get(&path).cloned() {
            if let Err(e) = write_left_file(state, runtime, &path, &left_content) {
                state.status_message = format!("Left write failed: {}", e);
                return;
            }
        }

        if runtime.is_side_available(&state.right_source) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::side::Side;
    use crate::runtime::RuntimeTargets;
    use crate::tree::{FileNode, FileTree};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn single_file_merge_stores_backup_outside_both_targets() {
        let left = TempDir::new().unwrap();
        let right = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        fs::write(left.path().join("file.txt"), "new\n").unwrap();
        fs::write(right.path().join("file.txt"), "old\n").unwrap();
        let config_path = left.path().join("config.toml");
        fs::write(
            &config_path,
            format!(
                "[local]\nroot_dir = {:?}\n[servers.develop]\nhost = \"example.invalid\"\nuser = \"unused\"\nroot_dir = {:?}\n[backup]\nenabled = true\n",
                left.path(), right.path()
            ),
        )
        .unwrap();
        let config = crate::config::load_config_from_paths(Some(&config_path), None).unwrap();
        let targets = RuntimeTargets::production()
            .with_local("develop", right.path())
            .with_backup_store(Some(store.path().to_path_buf()));
        let mut state = AppState::new(
            FileTree {
                root: left.path().into(),
                nodes: vec![FileNode::new_file("file.txt")],
            },
            FileTree {
                root: right.path().into(),
                nodes: vec![FileNode::new_file("file.txt")],
            },
            Side::Local,
            Side::Remote("develop".into()),
            "base16-ocean.dark",
        );
        state.left_cache.insert("file.txt".into(), "new\n".into());
        let mut runtime = TuiRuntime::with_targets(config, targets);

        execute_merge(
            &mut state,
            &mut runtime,
            &ConfirmDialog::new(
                "file.txt".into(),
                MergeDirection::LeftToRight,
                "local".into(),
                "develop".into(),
            ),
        );

        assert_eq!(
            fs::read_to_string(right.path().join("file.txt")).unwrap(),
            "new\n"
        );
        assert_eq!(fs::read_dir(right.path()).unwrap().count(), 1);
        assert!(fs::read_dir(store.path()).unwrap().next().is_some());
    }

    // ── format_merge_success ──

    #[test]
    fn test_format_merge_success_left_to_right() {
        let msg = format_merge_success("app.rs", "local", "remote", MergeDirection::LeftToRight);
        assert_eq!(msg, "app.rs: local -> remote merged");
    }

    #[test]
    fn test_format_merge_success_right_to_left() {
        let msg = format_merge_success("app.rs", "local", "remote", MergeDirection::RightToLeft);
        assert_eq!(msg, "app.rs: remote -> local merged");
    }

    // ── format_hunk_merge_success ──

    #[test]
    fn test_format_hunk_merge_success_right_to_left() {
        let msg =
            format_hunk_merge_success("local", "remote", "app.rs", HunkDirection::RightToLeft, 3);
        assert_eq!(msg, "Hunk merged: remote -> local (app.rs) | 3 hunks left");
    }

    #[test]
    fn test_format_hunk_merge_success_left_to_right() {
        let msg =
            format_hunk_merge_success("local", "remote", "app.rs", HunkDirection::LeftToRight, 0);
        assert_eq!(msg, "Hunk merged: local -> remote (app.rs) | 0 hunks left");
    }

    #[test]
    fn test_format_hunk_merge_success_zero_hunks() {
        let msg =
            format_hunk_merge_success("dev", "staging", "main.rs", HunkDirection::RightToLeft, 0);
        assert_eq!(msg, "Hunk merged: staging -> dev (main.rs) | 0 hunks left");
    }
}
