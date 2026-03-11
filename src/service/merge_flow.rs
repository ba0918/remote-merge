//! merge 実行の共通パイプライン。
//! cli/merge.rs と cli/sync.rs の両方から利用する。
//! I/O 操作を含むため、純粋関数ではない。

use crate::app::Side;
use crate::merge::executor::MergeDirection;
use crate::runtime::CoreRuntime;
use crate::service::merge::{determine_merge_action, MergeAction};
use crate::service::types::*;
use crate::tree::FileTree;

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
}

/// ソース側にファイルが存在しない方向のマージを検出する
///
/// - `LeftToRight` + `RightOnly` = ソース(left)にファイルがない
/// - `RightToLeft` + `LeftOnly` = ソース(right)にファイルがない
pub fn check_source_exists(
    path: &str,
    direction: MergeDirection,
    statuses: &[FileStatus],
) -> anyhow::Result<()> {
    let status = statuses.iter().find(|s| s.path == path);
    let source_missing = matches!(
        (direction, status.map(|s| &s.status)),
        (MergeDirection::LeftToRight, Some(FileStatusKind::RightOnly))
            | (MergeDirection::RightToLeft, Some(FileStatusKind::LeftOnly))
    );
    if source_missing {
        let source_name = match direction {
            MergeDirection::LeftToRight => "left (source)",
            MergeDirection::RightToLeft => "right (source)",
        };
        anyhow::bail!(
            "File '{}' does not exist on {} side. Cannot merge a non-existent source file.",
            path,
            source_name
        );
    }
    Ok(())
}

/// 単一ファイルのマージを実行する
pub fn execute_single_merge(
    ctx: &mut MergeContext<'_>,
    path: &str,
) -> anyhow::Result<MergeFileResult> {
    // ソース側にファイルが存在するか確認
    check_source_exists(path, ctx.direction, ctx.statuses)?;

    // ソース側・ターゲット側の決定
    let (source, target) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left, ctx.right),
        MergeDirection::RightToLeft => (ctx.right, ctx.left),
    };

    // ソース側・ターゲット側のツリーを決定
    let (source_tree, target_tree) = match ctx.direction {
        MergeDirection::LeftToRight => (ctx.left_tree, ctx.right_tree),
        MergeDirection::RightToLeft => (ctx.right_tree, ctx.left_tree),
    };

    // symlink 分岐を純粋関数で判定
    let action = determine_merge_action(source_tree, target_tree, path);

    match action {
        MergeAction::CreateSymlink {
            link_target,
            target_exists,
        } => {
            // ターゲット側に既存ファイル/symlink がある場合、バックアップを作成してから削除
            let backup_path = if target_exists && ctx.core.config.backup.enabled {
                let paths = vec![path.to_string()];
                match ctx.core.create_backups(target, &paths, ctx.session_id) {
                    Ok(()) => Some(format!("{}/{}", ctx.session_id, path)),
                    Err(e) => {
                        tracing::warn!("Backup failed (continuing): {}", e);
                        None
                    }
                }
            } else {
                None
            };
            if target_exists {
                ctx.core.remove_file(target, path)?;
            }
            ctx.core.create_symlink(target, path, &link_target)?;
            return Ok(MergeFileResult {
                path: path.to_string(),
                status: "ok".into(),
                backup: backup_path,
                ref_badge: None,
            });
        }
        MergeAction::ReplaceSymlinkWithFile => {
            // ターゲットが symlink でソースが通常ファイル → バックアップしてから symlink を削除
            // バックアップは symlink 削除前に行う（削除後ではバックアップ対象が存在しない）
            let symlink_backup = if ctx.core.config.backup.enabled {
                let paths = vec![path.to_string()];
                match ctx.core.create_backups(target, &paths, ctx.session_id) {
                    Ok(()) => Some(format!("{}/{}", ctx.session_id, path)),
                    Err(e) => {
                        tracing::warn!("Backup failed for symlink target (continuing): {}", e);
                        None
                    }
                }
            } else {
                None
            };
            ctx.core.remove_file(target, path)?;

            // symlink 削除後は通常ファイル書き込み — バックアップ済みなのでスキップ
            let content = ctx.core.read_file_bytes(source, path, ctx.force)?;
            ctx.core.write_file_bytes(target, path, &content)?;
            if ctx.with_permissions {
                copy_permissions(source, target, path, ctx.core);
            }
            return Ok(MergeFileResult {
                path: path.to_string(),
                status: "ok".into(),
                backup: symlink_backup,
                ref_badge: None,
            });
        }
        MergeAction::Normal => {
            // 通常マージ — 何もせずそのまま後続処理へ
        }
    }

    // バイト列でコンテンツ読み込み（ソース側） — バイナリファイルも破壊しない
    let content = ctx.core.read_file_bytes(source, path, ctx.force)?;

    // バックアップ（ターゲット側）
    let backup_path = if ctx.core.config.backup.enabled {
        let paths = vec![path.to_string()];
        match ctx.core.create_backups(target, &paths, ctx.session_id) {
            Ok(()) => Some(format!("{}/{}", ctx.session_id, path)),
            Err(e) => {
                tracing::warn!("Backup failed (continuing): {}", e);
                None
            }
        }
    } else {
        None
    };

    // バイト列で書き込み（ターゲット側） — バイナリファイルも破壊しない
    ctx.core.write_file_bytes(target, path, &content)?;

    // パーミッションコピー（--with-permissions 指定時）
    if ctx.with_permissions {
        copy_permissions(source, target, path, ctx.core);
    }

    Ok(MergeFileResult {
        path: path.to_string(),
        status: "ok".into(),
        backup: backup_path,
        ref_badge: None,
    })
}

/// ソースからターゲットへパーミッションをコピーする
pub fn copy_permissions(source: &Side, target: &Side, path: &str, core: &mut CoreRuntime) {
    let mode = match source {
        Side::Local => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let full = core.config.local.root_dir.join(path);
                std::fs::metadata(&full)
                    .map(|m| m.permissions().mode() & 0o777)
                    .ok()
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                None
            }
        }
        Side::Remote(_) => {
            // リモートの場合、CLI ではツリーデータがないため stat で取得が必要。
            // 現時点では未サポート（TUI 側では FileNode.permissions を使用）。
            None
        }
    };

    if let Some(m) = mode {
        if m > 0 && m <= 0o777 {
            if let Err(e) = core.chmod_file(target, path, m) {
                tracing::warn!("Failed to set permissions for {}: {}", path, e);
            }
        }
    }
}

/// 削除操作の実行（--delete 用）。
/// バックアップ失敗時は削除を中止して failed に記録する（安全設計）。
pub fn execute_deletions(
    core: &mut CoreRuntime,
    target: &Side,
    right_only_files: &[String],
    session_id: &str,
) -> (Vec<DeleteFileResult>, Vec<MergeFailure>) {
    let mut deleted = Vec::new();
    let mut failed = Vec::new();

    for path in right_only_files {
        // バックアップ（有効な場合）
        if core.config.backup.enabled {
            let paths = vec![path.clone()];
            if let Err(e) = core.create_backups(target, &paths, session_id) {
                // バックアップ失敗 → 削除を中止（安全設計）
                failed.push(MergeFailure {
                    path: path.clone(),
                    error: format!("Backup failed, deletion aborted: {}", e),
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

    (deleted, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── check_source_exists tests ──

    #[test]
    fn test_check_source_exists_left_to_right_right_only_fails() {
        // LeftToRight + RightOnly = ソース(left)にファイルがない → エラー
        let statuses = vec![FileStatus {
            path: "new_file.rs".into(),
            status: FileStatusKind::RightOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
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
        // RightToLeft + LeftOnly = ソース(right)にファイルがない → エラー
        let statuses = vec![FileStatus {
            path: "old_file.rs".into(),
            status: FileStatusKind::LeftOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
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
        // LeftToRight + LeftOnly = ソース(left)にファイルがある → OK
        let statuses = vec![FileStatus {
            path: "file.rs".into(),
            status: FileStatusKind::LeftOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        assert!(check_source_exists("file.rs", MergeDirection::LeftToRight, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_right_to_left_right_only_ok() {
        // RightToLeft + RightOnly = ソース(right)にファイルがある → OK
        let statuses = vec![FileStatus {
            path: "file.rs".into(),
            status: FileStatusKind::RightOnly,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
        assert!(check_source_exists("file.rs", MergeDirection::RightToLeft, &statuses).is_ok());
    }

    #[test]
    fn test_check_source_exists_modified_ok() {
        // Modified = 両側にある → どちらの方向でもOK
        let statuses = vec![FileStatus {
            path: "file.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        }];
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
}
