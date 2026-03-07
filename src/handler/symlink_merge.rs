//! シンボリックリンクのマージ実行。
//! ローカル: アトミック symlink 置換 (tmp + rename)、リモート: SSH exec (ln -sfn)。

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

    match direction {
        MergeDirection::LeftToRight => {
            if !state.is_connected {
                state.status_message = "SSH not connected: cannot merge symlink".to_string();
                return;
            }
            match runtime.create_remote_symlink(&state.server_name, path, &source_target) {
                Ok(()) => {
                    let left = state.left_source.display_name();
                    let right = state.right_source.display_name();
                    state.status_message = format!(
                        "{}: symlink {} -> {} merged (-> {})",
                        path, left, right, source_target
                    );
                }
                Err(e) => {
                    state.status_message = format!("Symlink merge failed: {}", e);
                }
            }
        }
        MergeDirection::RightToLeft => {
            match create_local_symlink(&state.left_tree.root, path, &source_target) {
                Ok(()) => {
                    let left = state.left_source.display_name();
                    let right = state.right_source.display_name();
                    state.status_message = format!(
                        "{}: symlink {} -> {} merged (-> {})",
                        path, right, left, source_target
                    );
                }
                Err(e) => {
                    state.status_message = format!("Symlink merge failed: {}", e);
                }
            }
        }
    }
}

/// ローカルにシンボリックリンクを作成/更新する（アトミック置換）
///
/// 一時パスにシンボリックリンクを作成し、rename() でアトミックに置換する。
/// これにより remove + symlink 間の TOCTOU 競合を回避する。
fn create_local_symlink(
    root: &std::path::Path,
    rel_path: &str,
    target: &str,
) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let full_path = root.join(rel_path);
        let tmp_path = full_path.with_file_name(format!(
            ".tmp_symlink_{}_{}",
            std::process::id(),
            full_path.file_name().unwrap_or_default().to_string_lossy()
        ));

        // 一時パスにシンボリックリンクを作成
        std::os::unix::fs::symlink(target, &tmp_path)?;

        // rename() でアトミックに置換（同一ファイルシステム上では原子的）
        if let Err(e) = std::fs::rename(&tmp_path, &full_path) {
            // rename 失敗時は一時ファイルをクリーンアップ
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e.into());
        }

        Ok(())
    }

    #[cfg(not(unix))]
    {
        let _ = (root, rel_path, target);
        anyhow::bail!("Symlink creation is only supported on Unix systems");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_local_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // ターゲットファイルを作成
        std::fs::write(root.join("target.txt"), "content").unwrap();

        // シンボリックリンクを作成
        create_local_symlink(root, "link.txt", "target.txt").unwrap();

        let link_path = root.join("link.txt");
        assert!(link_path
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        let actual_target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(actual_target.to_str().unwrap(), "target.txt");
    }

    #[test]
    fn test_create_local_symlink_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // 既存ファイルを作成
        std::fs::write(root.join("link.txt"), "old content").unwrap();

        // シンボリックリンクで上書き
        create_local_symlink(root, "link.txt", "new_target").unwrap();

        let link_path = root.join("link.txt");
        assert!(link_path
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn test_create_local_symlink_overwrites_existing_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // 既存シンボリックリンクを作成
        #[cfg(unix)]
        std::os::unix::fs::symlink("old_target", root.join("link.txt")).unwrap();

        // 新しいターゲットで上書き
        create_local_symlink(root, "link.txt", "new_target").unwrap();

        let actual_target = std::fs::read_link(root.join("link.txt")).unwrap();
        assert_eq!(actual_target.to_str().unwrap(), "new_target");
    }

    #[test]
    fn test_atomic_replace_no_temp_file_left() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // シンボリックリンクを作成
        create_local_symlink(root, "link.txt", "target_a").unwrap();

        // 一時ファイルが残っていないことを確認
        let entries: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "Only the symlink should exist, no temp files"
        );
        assert_eq!(entries[0].file_name().to_str().unwrap(), "link.txt");
    }

    #[test]
    fn test_atomic_replace_preserves_symlink_during_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // 初回作成
        create_local_symlink(root, "link.txt", "target_a").unwrap();

        // 上書き — rename() によるアトミック置換なので中間状態でリンクが消えない
        create_local_symlink(root, "link.txt", "target_b").unwrap();

        let link_path = root.join("link.txt");
        // シンボリックリンクが存在する
        assert!(link_path
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        // ターゲットが更新されている
        let actual = std::fs::read_link(&link_path).unwrap();
        assert_eq!(actual.to_str().unwrap(), "target_b");

        // 一時ファイルが残っていない
        let count = std::fs::read_dir(root).unwrap().count();
        assert_eq!(count, 1);
    }
}
