//! merge サービス: ファイルマージの結果型組み立て。
//!
//! 実際のI/O操作（SSH書き込み・バックアップ）は CLI 層が CoreRuntime 経由で行う。
//! このモジュールは結果の組み立てと dry-run 判定のみ。

use super::status::is_sensitive;
use super::types::*;
use crate::app::side::is_remote_to_remote;
use crate::app::Side;
use crate::tree::{FileTree, NodeKind};

/// マージ対象ファイルの前処理結果
#[derive(Debug, Clone)]
pub struct MergePlan {
    /// マージ対象ファイル
    pub files: Vec<String>,
    /// スキップ対象（センシティブ等）
    pub skipped: Vec<MergeSkipped>,
}

/// マージ対象をフィルタリングし、MergePlan を構築する（純粋関数）。
///
/// センシティブファイルは `--force` なしではスキップする。
pub fn plan_merge(paths: &[String], sensitive_patterns: &[String], force: bool) -> MergePlan {
    let mut files = Vec::new();
    let mut skipped = Vec::new();

    for path in paths {
        if !force && is_sensitive(path, sensitive_patterns) {
            skipped.push(MergeSkipped {
                path: path.clone(),
                reason: "sensitive file".into(),
            });
        } else {
            files.push(path.clone());
        }
    }

    MergePlan { files, skipped }
}

/// マージ結果を組み立てる（純粋関数）。
pub fn build_merge_output(
    merged: Vec<MergeFileResult>,
    skipped: Vec<MergeSkipped>,
    deleted: Vec<DeleteFileResult>,
    failed: Vec<MergeFailure>,
    ref_info: Option<SourceInfo>,
) -> MergeOutput {
    MergeOutput {
        merged,
        skipped,
        deleted,
        failed,
        ref_: ref_info,
    }
}

/// merge の exit code を判定する。
pub fn merge_exit_code(output: &MergeOutput) -> i32 {
    if output.failed.is_empty() {
        exit_code::SUCCESS
    } else {
        exit_code::ERROR
    }
}

/// ツリーからパスに対応する symlink のターゲットを取得する純粋関数
pub fn find_symlink_target(tree: &FileTree, path: &str) -> Option<String> {
    let node = tree.find_node(path)?;
    match &node.kind {
        NodeKind::Symlink { target } => Some(target.clone()),
        _ => None,
    }
}

/// symlink merge のアクション判定結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeAction {
    /// ソースが symlink → ターゲットに symlink を作成
    CreateSymlink {
        link_target: String,
        /// ターゲット側にファイルまたは symlink が存在するか（ディレクトリは除外）
        target_exists: bool,
    },
    /// ターゲットが symlink でソースが通常ファイル → symlink を削除してからファイル書き込み
    ReplaceSymlinkWithFile,
    /// 通常のファイルマージ（symlink なし）
    Normal,
}

/// ソース/ターゲットのツリーとパスからマージアクションを決定する純粋関数
///
/// - ソースが symlink → `CreateSymlink`
/// - ターゲットが symlink でソースが通常ファイル → `ReplaceSymlinkWithFile`
/// - どちらも symlink でない → `Normal`
pub fn determine_merge_action(
    source_tree: &FileTree,
    target_tree: &FileTree,
    path: &str,
) -> MergeAction {
    if let Some(link_target) = find_symlink_target(source_tree, path) {
        // ターゲット側にファイル/symlink が存在するか（ディレクトリは remove_file で削除できないため除外）
        // create_symlink は内部で ln -sfn（リモート）/ remove_file + symlink（ローカル）を行うため、
        // ファイル/symlink の上書きは安全に処理される
        let target_exists = target_tree
            .find_node(path)
            .is_some_and(|node| !matches!(node.kind, NodeKind::Directory));
        return MergeAction::CreateSymlink {
            link_target,
            target_exists,
        };
    }
    if find_symlink_target(target_tree, path).is_some() {
        return MergeAction::ReplaceSymlinkWithFile;
    }
    MergeAction::Normal
}

/// remote-to-remote merge のガード判定。
/// ブロックされた場合は MergeOutcome::R2rBlocked を返す。
pub fn check_r2r_guard(
    left: &Side,
    right: &Side,
    dry_run: bool,
    force: bool,
) -> Option<MergeOutcome> {
    if is_remote_to_remote(left, right) && !dry_run && !force {
        Some(MergeOutcome::R2rBlocked {
            left: left.display_name().to_string(),
            right: right.display_name().to_string(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_merge_skips_sensitive() {
        let paths = vec![".env".into(), "src/main.rs".into(), "secret.pem".into()];
        let patterns = vec![".env".into(), "*.pem".into()];
        let plan = plan_merge(&paths, &patterns, false);

        assert_eq!(plan.files, vec!["src/main.rs"]);
        assert_eq!(plan.skipped.len(), 2);
        assert_eq!(plan.skipped[0].path, ".env");
        assert_eq!(plan.skipped[1].path, "secret.pem");
    }

    #[test]
    fn test_plan_merge_force_includes_sensitive() {
        let paths = vec![".env".into(), "src/main.rs".into()];
        let patterns = vec![".env".into()];
        let plan = plan_merge(&paths, &patterns, true);

        assert_eq!(plan.files.len(), 2);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn test_plan_merge_empty() {
        let plan = plan_merge(&[], &[], false);
        assert!(plan.files.is_empty());
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn test_build_merge_output() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
                hunks_applied: None,
                hunks_total: None,
                direction: None,
            }],
            vec![],
            vec![],
            vec![],
            None,
        );
        assert_eq!(output.merged.len(), 1);
        assert!(output.failed.is_empty());
    }

    #[test]
    fn test_merge_exit_code_success() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
                hunks_applied: None,
                hunks_total: None,
                direction: None,
            }],
            vec![],
            vec![],
            vec![],
            None,
        );
        assert_eq!(merge_exit_code(&output), exit_code::SUCCESS);
    }

    #[test]
    fn test_merge_exit_code_failure() {
        let output = build_merge_output(
            vec![],
            vec![],
            vec![],
            vec![MergeFailure {
                path: "a.rs".into(),
                error: "write error".into(),
            }],
            None,
        );
        assert_eq!(merge_exit_code(&output), exit_code::ERROR);
    }

    #[test]
    fn test_build_merge_output_with_ref() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: Some("differs".into()),
                hunks_applied: None,
                hunks_total: None,
                direction: None,
            }],
            vec![],
            vec![],
            vec![],
            Some(SourceInfo {
                label: "staging".into(),
                root: "/s".into(),
            }),
        );
        assert!(output.ref_.is_some());
        assert_eq!(output.ref_.as_ref().unwrap().label, "staging");
        assert_eq!(output.merged[0].ref_badge.as_ref().unwrap(), "differs");
    }

    #[test]
    fn test_build_merge_output_no_ref_backward_compat() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
                hunks_applied: None,
                hunks_total: None,
                direction: None,
            }],
            vec![],
            vec![],
            vec![],
            None,
        );
        assert!(output.ref_.is_none());
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("\"ref\""));
        assert!(!json.contains("\"ref_badge\""));
    }

    #[test]
    fn test_build_merge_output_with_deleted() {
        let output = build_merge_output(
            vec![],
            vec![],
            vec![DeleteFileResult {
                path: "old.txt".into(),
                status: DeleteStatus::Ok,
                backup: Some("session/old.txt".into()),
            }],
            vec![],
            None,
        );
        assert_eq!(output.deleted.len(), 1);
        assert_eq!(output.deleted[0].path, "old.txt");
    }

    #[test]
    fn test_build_merge_output_deleted_empty_backward_compat() {
        let output = build_merge_output(
            vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
                hunks_applied: None,
                hunks_total: None,
                direction: None,
            }],
            vec![],
            vec![],
            vec![],
            None,
        );
        let json = serde_json::to_string(&output).unwrap();
        // deleted は空でも常に JSON に含まれる
        assert!(json.contains("\"deleted\""));
    }

    // ── find_symlink_target tests ──

    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_tree_with_nodes(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    #[test]
    fn test_find_symlink_target_returns_target_for_symlink() {
        let tree = make_tree_with_nodes(vec![FileNode::new_symlink("link.txt", "/real/target")]);
        let result = find_symlink_target(&tree, "link.txt");
        assert_eq!(result, Some("/real/target".to_string()));
    }

    #[test]
    fn test_find_symlink_target_returns_none_for_regular_file() {
        let tree = make_tree_with_nodes(vec![FileNode::new_file("regular.txt")]);
        let result = find_symlink_target(&tree, "regular.txt");
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_symlink_target_returns_none_for_directory() {
        let tree = make_tree_with_nodes(vec![FileNode::new_dir_with_children("mydir", vec![])]);
        let result = find_symlink_target(&tree, "mydir");
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_symlink_target_returns_none_for_missing_path() {
        let tree = make_tree_with_nodes(vec![FileNode::new_file("exists.txt")]);
        let result = find_symlink_target(&tree, "nonexistent.txt");
        assert_eq!(result, None);
    }

    // ── determine_merge_action tests ──

    #[test]
    fn test_determine_merge_action_source_symlink_target_regular_file() {
        let source_tree =
            make_tree_with_nodes(vec![FileNode::new_symlink("app.conf", "/etc/app.conf")]);
        let target_tree = make_tree_with_nodes(vec![FileNode::new_file("app.conf")]);

        let action = determine_merge_action(&source_tree, &target_tree, "app.conf");
        assert_eq!(
            action,
            MergeAction::CreateSymlink {
                link_target: "/etc/app.conf".to_string(),
                target_exists: true,
            }
        );
    }

    #[test]
    fn test_determine_merge_action_source_symlink_target_not_exists() {
        let source_tree =
            make_tree_with_nodes(vec![FileNode::new_symlink("link.txt", "/tmp/data")]);
        let target_tree = make_tree_with_nodes(vec![]);

        let action = determine_merge_action(&source_tree, &target_tree, "link.txt");
        assert_eq!(
            action,
            MergeAction::CreateSymlink {
                link_target: "/tmp/data".to_string(),
                target_exists: false,
            }
        );
    }

    #[test]
    fn test_determine_merge_action_source_regular_file_target_symlink() {
        let source_tree = make_tree_with_nodes(vec![FileNode::new_file("config.yml")]);
        let target_tree =
            make_tree_with_nodes(vec![FileNode::new_symlink("config.yml", "/old/target")]);

        let action = determine_merge_action(&source_tree, &target_tree, "config.yml");
        assert_eq!(action, MergeAction::ReplaceSymlinkWithFile);
    }

    #[test]
    fn test_determine_merge_action_both_symlinks() {
        let source_tree = make_tree_with_nodes(vec![FileNode::new_symlink("link", "/new/target")]);
        let target_tree = make_tree_with_nodes(vec![FileNode::new_symlink("link", "/old/target")]);

        let action = determine_merge_action(&source_tree, &target_tree, "link");
        assert_eq!(
            action,
            MergeAction::CreateSymlink {
                link_target: "/new/target".to_string(),
                target_exists: true,
            }
        );
    }

    #[test]
    fn test_determine_merge_action_both_symlinks_same_target() {
        let source_tree =
            make_tree_with_nodes(vec![FileNode::new_symlink("link", "/shared/target")]);
        let target_tree =
            make_tree_with_nodes(vec![FileNode::new_symlink("link", "/shared/target")]);

        let action = determine_merge_action(&source_tree, &target_tree, "link");
        assert_eq!(
            action,
            MergeAction::CreateSymlink {
                link_target: "/shared/target".to_string(),
                target_exists: true,
            }
        );
    }

    #[test]
    fn test_determine_merge_action_both_regular_files() {
        let source_tree = make_tree_with_nodes(vec![FileNode::new_file("data.txt")]);
        let target_tree = make_tree_with_nodes(vec![FileNode::new_file("data.txt")]);

        let action = determine_merge_action(&source_tree, &target_tree, "data.txt");
        assert_eq!(action, MergeAction::Normal);
    }

    #[test]
    fn test_determine_merge_action_source_only_regular_file() {
        let source_tree = make_tree_with_nodes(vec![FileNode::new_file("new_file.txt")]);
        let target_tree = make_tree_with_nodes(vec![]);

        let action = determine_merge_action(&source_tree, &target_tree, "new_file.txt");
        assert_eq!(action, MergeAction::Normal);
    }

    #[test]
    fn test_determine_merge_action_source_not_found() {
        let source_tree = make_tree_with_nodes(vec![]);
        let target_tree = make_tree_with_nodes(vec![]);

        let action = determine_merge_action(&source_tree, &target_tree, "missing.txt");
        assert_eq!(action, MergeAction::Normal);
    }

    #[test]
    fn test_determine_merge_action_source_symlink_target_is_directory() {
        let source_tree = make_tree_with_nodes(vec![FileNode::new_symlink("logs", "/var/log/app")]);
        let target_tree =
            make_tree_with_nodes(vec![FileNode::new_dir_with_children("logs", vec![])]);

        let action = determine_merge_action(&source_tree, &target_tree, "logs");
        assert_eq!(
            action,
            MergeAction::CreateSymlink {
                link_target: "/var/log/app".to_string(),
                target_exists: false,
            }
        );
    }

    // ── check_r2r_guard tests ──

    #[test]
    fn test_check_r2r_guard_blocks_remote_to_remote() {
        let left = Side::Remote("develop".into());
        let right = Side::Remote("staging".into());
        let result = check_r2r_guard(&left, &right, false, false);
        assert!(matches!(result, Some(MergeOutcome::R2rBlocked { .. })));
    }

    #[test]
    fn test_check_r2r_guard_allows_with_force() {
        let left = Side::Remote("develop".into());
        let right = Side::Remote("staging".into());
        assert!(check_r2r_guard(&left, &right, false, true).is_none());
    }

    #[test]
    fn test_check_r2r_guard_allows_with_dry_run() {
        let left = Side::Remote("develop".into());
        let right = Side::Remote("staging".into());
        assert!(check_r2r_guard(&left, &right, true, false).is_none());
    }

    #[test]
    fn test_check_r2r_guard_allows_local_to_remote() {
        let left = Side::Local;
        let right = Side::Remote("staging".into());
        assert!(check_r2r_guard(&left, &right, false, false).is_none());
    }
}
