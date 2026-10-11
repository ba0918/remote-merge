//! merge 実行の共通パイプライン。
//! cli/merge.rs と cli/sync.rs の両方から利用する。
//! I/O 操作を含むため、純粋関数ではない。

use std::collections::HashMap;
use std::path::Path;

use crate::merge::executor::MergeDirection;
use crate::runtime::change_check::Before;
use crate::runtime::confirm::ConfirmedChange;
use crate::runtime::CoreRuntime;
use crate::service::merge::{determine_merge_action, MergeAction, DIFFERENT_KIND_REASON};
use crate::service::permission_copy::permissions_to_copy;
use crate::service::types::*;
use crate::side::Side;
use crate::tree::FileTree;
use remote_merge_engine::service::merge_flow::{prepare_hunk_merge, HunkMergePreparation};

pub use remote_merge_engine::service::merge_flow::{
    check_source_exists, check_symlink_destination_unchanged, validate_hunk_merge_target,
};

/// 単一マージに必要なコンテキスト
pub struct MergeContext<'a> {
    pub left: &'a Side,
    pub right: &'a Side,
    pub left_tree: &'a FileTree,
    pub right_tree: &'a FileTree,
    pub direction: MergeDirection,
    pub core: &'a mut CoreRuntime,
    pub with_permissions: bool,
    pub force: bool,
    pub statuses: &'a [FileStatus],
    pub session_id: &'a str,
    pub expected_target_contents: &'a HashMap<String, Vec<u8>>,
}

pub enum SingleMergeResult {
    Merged(MergeFileResult),
    Skipped(MergeSkipped),
}

/// 単一ファイルのマージを実行する
pub fn execute_single_merge(
    ctx: &mut MergeContext<'_>,
    path: &str,
) -> anyhow::Result<SingleMergeResult> {
    // ソース側にファイルが存在するか確認
    check_source_exists(path, ctx.direction, ctx.statuses)?;

    // ソース側・ターゲット側の決定
    let (source, target) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left, ctx.right),
        MergeDirection::RightToLeft => (ctx.right, ctx.left),
    };

    // `接続断` で止めた書き込み先には、手を付けない
    if let Some(stopped) = ctx.core.stopped_error_for(target) {
        return Err(stopped);
    }

    // ソース側・ターゲット側のツリーを決定
    let (source_tree, target_tree) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left_tree, ctx.right_tree),
        MergeDirection::RightToLeft => (ctx.right_tree, ctx.left_tree),
    };

    // symlink 分岐を純粋関数で判定
    let action = determine_merge_action(source_tree, target_tree, path);

    match action {
        MergeAction::SkipDifferentKind => {
            return Ok(SingleMergeResult::Skipped(MergeSkipped {
                path: path.to_string(),
                reason: DIFFERENT_KIND_REASON.into(),
            }));
        }
        MergeAction::CreateSymlink {
            link_target,
            target_exists,
        } => {
            // 比べたときにあった symlink は、比べたときのまま残っているときだけ置き換える
            if target_exists {
                let current = ctx.core.current_link_target(target, path)?;
                check_symlink_destination_unchanged(path, target_tree, current.as_deref())?;
            }
            // ターゲット側に既存ファイル/symlink がある場合、バックアップを作成してから削除
            let backup_path = if ctx.core.config.backup.enabled {
                if target_exists {
                    ctx.core
                        .save_symlink_update(target, path, ctx.session_id, &link_target)
                        .map_err(backup_failure)?
                } else {
                    None
                }
            } else {
                None
            };
            let missing_directories = ctx.core.missing_parent_directories(target, path)?;
            if target_exists {
                ctx.core.remove_file(target, path)?;
                ctx.core
                    .create_symlink(target, path, &link_target)
                    .map_err(existing_entry_deleted)?;
            } else {
                ctx.core.create_symlink(target, path, &link_target)?;
            }
            set_directory_permissions(ctx.core, target, &missing_directories)
                .map_err(link_created)?;
            return Ok(SingleMergeResult::Merged(MergeFileResult {
                path: path.to_string(),
                status: "ok".into(),
                backup: backup_path,
                ref_badge: None,
                hunk_info: None,
            }));
        }
        MergeAction::ReplaceSymlinkWithFile => {
            // ターゲットが symlink でソースが通常ファイル → バックアップしてから symlink を削除
            // バックアップは symlink 削除前に行う（削除後ではバックアップ対象が存在しない）
            let symlink_backup = if ctx.core.config.backup.enabled {
                Some(
                    ctx.core
                        .save_backup(target, path, ctx.session_id, ctx.force)
                        .map_err(backup_failure)?,
                )
            } else {
                None
            };
            ctx.core.remove_file(target, path)?;

            // symlink 削除後は通常ファイル書き込み — バックアップ済みなのでスキップ
            let content = ctx.core.read_file_bytes(source, path, ctx.force)?;
            ctx.core
                .write_file_bytes(target, path, &content, &Before::Absent)
                .map_err(existing_entry_deleted)?;
            if ctx.with_permissions {
                copy_permissions(source_tree, target, path, ctx.core).map_err(content_written)?;
            }
            return Ok(SingleMergeResult::Merged(MergeFileResult {
                path: path.to_string(),
                status: "ok".into(),
                backup: symlink_backup,
                ref_badge: None,
                hunk_info: None,
            }));
        }
        MergeAction::Normal => {
            // 通常マージ — 何もせずそのまま後続処理へ
        }
    }

    // バイト列でコンテンツ読み込み（ソース側） — バイナリファイルも破壊しない
    let content = ctx.core.read_file_bytes(source, path, ctx.force)?;

    // 書く前の状態は、書き込みの最中の `接続断` の後に書き込み先を確かめるのに使う
    let before = if target_tree
        .find_node(Path::new(path))
        .is_some_and(|node| node.is_file())
    {
        let expected = ctx
            .expected_target_contents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("destination was not read before merge: {path}"))?;
        let actual = ctx.core.read_file_bytes(target, path, ctx.force)?;
        if &actual != expected {
            anyhow::bail!("destination content changed since comparison: {path}");
        }
        Before::Content(actual)
    } else if target_tree.find_node(Path::new(path)).is_none()
        && ctx.core.target_path_exists(target, path)?
    {
        anyhow::bail!("destination appeared since comparison: {path}");
    } else {
        Before::Absent
    };

    // バックアップ（ターゲット側）
    let backup_path = if ctx.core.config.backup.enabled {
        ctx.core
            .save_backup_if_exists(target, path, ctx.session_id, ctx.force)
            .map_err(backup_failure)?
    } else {
        None
    };

    let missing_directories = ctx.core.missing_parent_directories(target, path)?;

    // バイト列で書き込み（ターゲット側） — バイナリファイルも破壊しない
    ctx.core
        .write_file_bytes(target, path, &content, &before)
        .map_err(|error| ConfirmedChange::with_backup(error, backup_path.as_deref()))?;

    set_directory_permissions(ctx.core, target, &missing_directories).map_err(content_written)?;

    if target_tree.find_node(Path::new(path)).is_none() && !ctx.with_permissions {
        let mode = match target {
            Side::Local => ctx.core.config.defaults.file_permissions,
            Side::Remote(name) => {
                let server = &ctx.core.config.servers[name];
                crate::config::resolve_file_permissions(server, &ctx.core.config.defaults)
            }
        };
        ctx.core
            .chmod_file(target, path, mode)
            .map_err(content_written)?;
    }

    // パーミッションコピー（--with-permissions 指定時）
    if ctx.with_permissions {
        copy_permissions(source_tree, target, path, ctx.core).map_err(content_written)?;
    }

    Ok(SingleMergeResult::Merged(MergeFileResult {
        path: path.to_string(),
        status: "ok".into(),
        backup: backup_path,
        ref_badge: None,
        hunk_info: None,
    }))
}

/// バックアップの失敗の文言（"backup failed: " に続けて原因）。種類は変えない
fn backup_failure(error: anyhow::Error) -> anyhow::Error {
    crate::runtime::connection::prefixed(error, "backup failed: ")
}

/// 書き込み先にあったものを消した後の失敗の文言。消したことを示す。種類は変えない
fn existing_entry_deleted(error: anyhow::Error) -> anyhow::Error {
    crate::runtime::connection::prefixed(error, "existing entry deleted, but ")
}

/// 中身を書いた後の失敗の文言。中身は書いたことを示す（REQ-merge-037・REQ-ssh-022）。種類は変えない
fn content_written(error: anyhow::Error) -> anyhow::Error {
    crate::runtime::connection::prefixed(error, "content written, but ")
}

/// symlink を作った後の失敗の文言。リンクは作ったことを示す。種類は変えない
fn link_created(error: anyhow::Error) -> anyhow::Error {
    crate::runtime::connection::prefixed(error, "link created, but ")
}

/// 書き込みで作られたディレクトリ `directories` に、書き込み先に設定された権限を付ける
/// （REQ-merge-012。どの経路でもここで付ける）
fn set_directory_permissions(
    core: &mut CoreRuntime,
    target: &Side,
    directories: &[String],
) -> anyhow::Result<()> {
    if directories.is_empty() {
        return Ok(());
    }
    let mode = match target {
        Side::Local => core.config.defaults.dir_permissions,
        Side::Remote(name) => {
            let server = &core.config.servers[name];
            crate::config::resolve_dir_permissions(server, &core.config.defaults)
        }
    };
    for directory in directories {
        core.chmod_file(target, directory, mode)?;
    }
    Ok(())
}

/// 読み込み元の走査したツリー `source_tree` の権限を、書き込み先の `path` に写す。
/// 写せない権限（値がない・特別なビットを含む）は写さずに Err にする（REQ-merge-037）
fn copy_permissions(
    source_tree: &FileTree,
    target: &Side,
    path: &str,
    core: &mut CoreRuntime,
) -> anyhow::Result<()> {
    let scanned = source_tree
        .find_node(Path::new(path))
        .and_then(|node| node.permissions);
    let mode = permissions_to_copy(scanned)?;
    core.chmod_file(target, path, mode)
}

/// 削除操作の実行（--delete 用）。
/// バックアップ失敗時は削除を中止して failed に記録する（安全設計）。
/// `force` はコマンドの --force で、バックアップのための読み込みの大きさの上限を外す。
pub fn execute_deletions(
    core: &mut CoreRuntime,
    target: &Side,
    right_only_files: &[String],
    session_id: &str,
    force: bool,
) -> (Vec<DeleteFileResult>, Vec<MergeSkipped>, Vec<MergeFailure>) {
    let mut deleted = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();

    for path in right_only_files {
        // `接続断` で止めた書き込み先には、手を付けない
        if let Some(stopped) = core.stopped_error_for(target) {
            failed.push(MergeFailure {
                path: path.clone(),
                error: stopped.to_string(),
            });
            continue;
        }
        match core.is_terminal_symlink(target, path) {
            Ok(true) => {
                skipped.push(MergeSkipped {
                    path: path.clone(),
                    reason: "destination is a symlink; deletion skipped".into(),
                });
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                failed.push(MergeFailure {
                    path: path.clone(),
                    error: format!("cannot inspect destination: {e}"),
                });
                continue;
            }
        }
        // バックアップ（有効な場合）
        if core.config.backup.enabled {
            if let Err(e) = core.save_backup(target, path, session_id, force) {
                // バックアップ失敗 → 削除を中止（安全設計）
                failed.push(MergeFailure {
                    path: path.clone(),
                    error: format!("backup failed: {}", e),
                });
                continue;
            }
        }

        // 削除実行
        match core.remove_file(target, path) {
            Ok(()) => {
                deleted.push(DeleteFileResult {
                    path: path.clone(),
                    status: DeleteStatus::Ok,
                    backup: if core.config.backup.enabled {
                        Some(format!("{}/{}", session_id, path))
                    } else {
                        None
                    },
                });
            }
            Err(e) => {
                failed.push(MergeFailure {
                    path: path.clone(),
                    error: format!("Delete failed: {}", e),
                });
            }
        }
    }

    (deleted, skipped, failed)
}

/// hunk merge のコンテキスト
pub struct HunkMergeContext<'a> {
    pub left: &'a Side,
    pub right: &'a Side,
    pub left_tree: &'a FileTree,
    pub right_tree: &'a FileTree,
    pub direction: MergeDirection,
    pub core: &'a mut CoreRuntime,
    pub force: bool,
    pub session_id: &'a str,
}

/// hunk 単位マージを実行する。
///
/// 引数バリデーション済みの値を受け取り、以下を行う:
/// 1. ファイル種別バリデーション（symlink）
/// 2. 両側のファイル内容取得
/// 3. バイナリチェック
/// 4. diff 計算 → hunk 取得
/// 5. hunk インデックス検証
/// 6. apply_selected_hunks_single_pass で選択的マージ
/// 7. バックアップ + ファイル書き込み（dry_run=false の場合）
pub fn execute_hunk_merge(
    ctx: &mut HunkMergeContext<'_>,
    path: &str,
    hunk_indices: &[usize],
    dry_run: bool,
) -> anyhow::Result<MergeFileResult> {
    // ソース側・ターゲット側の決定
    let (source, target) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left, ctx.right),
        MergeDirection::RightToLeft => (ctx.right, ctx.left),
    };
    let (source_tree, target_tree) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left_tree, ctx.right_tree),
        MergeDirection::RightToLeft => (ctx.right_tree, ctx.left_tree),
    };

    // symlink バリデーション
    validate_hunk_merge_target(path, source_tree, target_tree)?;

    // 両側の内容を取得
    let source_bytes = ctx.core.read_file_bytes(source, path, ctx.force)?;
    let target_bytes = ctx.core.read_file_bytes(target, path, ctx.force)?;

    match prepare_hunk_merge(
        path,
        &source_bytes,
        &target_bytes,
        ctx.direction,
        hunk_indices,
    )? {
        HunkMergePreparation::Equal => Ok(MergeFileResult {
            path: path.to_string(),
            status: "skipped (no changes)".into(),
            ..Default::default()
        }),
        HunkMergePreparation::Modified {
            merged_text,
            hunk_info,
        } => {
            let hunk_info = Some(hunk_info);

            if dry_run {
                return Ok(MergeFileResult {
                    path: path.to_string(),
                    status: "would merge".into(),
                    hunk_info,
                    ..Default::default()
                });
            }

            // バックアップ
            let backup_path = if ctx.core.config.backup.enabled {
                Some(
                    ctx.core
                        .save_backup(target, path, ctx.session_id, ctx.force)
                        .map_err(backup_failure)?,
                )
            } else {
                None
            };

            // 書き込み
            ctx.core
                .write_file_bytes(
                    target,
                    path,
                    merged_text.as_bytes(),
                    &Before::Content(target_bytes),
                )
                .map_err(|error| ConfirmedChange::with_backup(error, backup_path.as_deref()))?;

            Ok(MergeFileResult {
                path: path.to_string(),
                status: "merged".into(),
                backup: backup_path,
                hunk_info,
                ..Default::default()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_status(path: &str, kind: FileStatusKind) -> FileStatus {
        FileStatus {
            path: path.to_string(),
            status: kind,
            hunks: None,
            ref_badge: None,
        }
    }

    // ── check_source_exists tests ──

    #[test]
    fn test_check_source_exists_left_to_right_right_only_fails() {
        // LeftToRight + RightOnly = ソース(left)にファイルがない → エラー
        let statuses = vec![make_status("new_file.rs", FileStatusKind::RightOnly)];
        let err = check_source_exists("new_file.rs", MergeDirection::LeftToRight, &statuses);
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(
            msg.contains("does not exist on left (source) side"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_check_source_exists_right_to_left_left_only_fails() {
        let statuses = vec![make_status("old_file.rs", FileStatusKind::LeftOnly)];
        let err = check_source_exists("old_file.rs", MergeDirection::RightToLeft, &statuses);
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(
            msg.contains("does not exist on right (source) side"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_check_source_exists_left_to_right_left_only_ok() {
        let statuses = vec![make_status("file.rs", FileStatusKind::LeftOnly)];
        assert!(check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_right_to_left_right_only_ok() {
        let statuses = vec![make_status("file.rs", FileStatusKind::RightOnly)];
        assert!(check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_modified_ok() {
        let statuses = vec![make_status("file.rs", FileStatusKind::Modified)];
        assert!(check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).is_ok());
        assert!(check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_unknown_path_ok() {
        // ステータスに存在しないパス → チェックをスキップ（OK扱い）
        let statuses = vec![];
        assert!(check_source_exists("unknown.rs", MergeDirection::LeftToRight, &statuses).is_ok());
    }

    // ── execute_deletions tests ──
    // execute_deletions は CoreRuntime の I/O メソッドを使用するため、
    // 統合テストで検証する。純粋関数部分（check_source_exists）はここでカバー。

    // ── additional check_source_exists tests ──

    #[test]
    fn test_check_source_exists_equal_ok() {
        let statuses = vec![make_status("file.rs", FileStatusKind::Equal)];
        assert!(check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).is_ok());
        assert!(check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_multiple_statuses() {
        let statuses = vec![
            make_status("ok.rs", FileStatusKind::Modified),
            make_status("bad.rs", FileStatusKind::RightOnly),
            make_status("also_ok.rs", FileStatusKind::LeftOnly),
        ];
        // ok.rs は Modified なので両方向 OK
        assert!(check_source_exists("ok.rs", MergeDirection::LeftToRight, &statuses).is_ok());
        // bad.rs は RightOnly なので LeftToRight はエラー
        assert!(check_source_exists("bad.rs", MergeDirection::LeftToRight, &statuses).is_err());
        // bad.rs は RightOnly なので RightToLeft は OK（ソースは right）
        assert!(check_source_exists("bad.rs", MergeDirection::RightToLeft, &statuses).is_ok());
        // also_ok.rs は LeftOnly なので LeftToRight は OK
        assert!(check_source_exists("also_ok.rs", MergeDirection::LeftToRight, &statuses).is_ok());
        // also_ok.rs は LeftOnly なので RightToLeft はエラー
        assert!(check_source_exists("also_ok.rs", MergeDirection::RightToLeft, &statuses).is_err());
    }

    #[test]
    fn test_check_source_error_message_left() {
        let statuses = vec![make_status("file.rs", FileStatusKind::RightOnly)];
        let err =
            check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("left (source)"),
            "expected 'left (source)' in: {}",
            msg
        );
    }

    #[test]
    fn test_check_source_error_message_right() {
        let statuses = vec![make_status("file.rs", FileStatusKind::LeftOnly)];
        let err =
            check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("right (source)"),
            "expected 'right (source)' in: {}",
            msg
        );
    }

    #[test]
    fn test_check_source_exists_with_hunks() {
        // hunks フィールドは check_source_exists には影響しない
        let mut status = make_status("file.rs", FileStatusKind::Modified);
        status.hunks = Some(5);
        let statuses = vec![status];
        assert!(check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).is_ok());
        assert!(check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).is_ok());
    }

    // ── validate_hunk_merge_target tests ──

    use crate::tree::{FileNode, NodeKind};
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_tree_with_file(path: &str) -> FileTree {
        let mut tree = FileTree::new(PathBuf::from("/tmp"));
        tree.nodes.push(FileNode {
            name: path.to_string(),
            kind: NodeKind::File,
            size: Some(100),
            mtime: Some(Utc::now()),
            children: None,
            permissions: None,
            link_is_dir: false,
            mtime_digits: crate::tree::NANOS_DIGITS,
        });
        tree
    }

    fn make_tree_with_symlink(path: &str) -> FileTree {
        let mut tree = FileTree::new(PathBuf::from("/tmp"));
        tree.nodes.push(FileNode {
            name: path.to_string(),
            kind: NodeKind::Symlink {
                target: "../link".into(),
            },
            size: None,
            mtime: None,
            children: None,
            permissions: None,
            link_is_dir: false,
            mtime_digits: crate::tree::NANOS_DIGITS,
        });
        tree
    }

    #[test]
    fn test_validate_hunk_merge_target_normal_file_ok() {
        let source = make_tree_with_file("src/foo.rs");
        let target = make_tree_with_file("src/foo.rs");
        assert!(validate_hunk_merge_target("src/foo.rs", &source, &target).is_ok());
    }

    #[test]
    fn test_validate_hunk_merge_target_source_symlink_error() {
        let source = make_tree_with_symlink("link.txt");
        let target = make_tree_with_file("link.txt");
        let err = validate_hunk_merge_target("link.txt", &source, &target);
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("symlink"), "unexpected error: {}", msg);
    }

    #[test]
    fn test_validate_hunk_merge_target_target_symlink_error() {
        let source = make_tree_with_file("link.txt");
        let target = make_tree_with_symlink("link.txt");
        let err = validate_hunk_merge_target("link.txt", &source, &target);
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("symlink"), "unexpected error: {}", msg);
    }

    #[test]
    fn test_validate_hunk_merge_target_unknown_path_ok() {
        // ツリーに存在しないパス → symlink チェックはスキップ（OK 扱い）
        let source = FileTree::new(PathBuf::from("/tmp"));
        let target = FileTree::new(PathBuf::from("/tmp"));
        assert!(validate_hunk_merge_target("nonexistent.rs", &source, &target).is_ok());
    }
}
