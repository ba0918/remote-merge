//! シンボリックリンクのマージ実行。
//! ローカル: std::fs::remove + symlink、リモート: SSH exec (ln -sfn)。

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
            MergeDirection::LocalToRemote => left_target.clone(),
            MergeDirection::RemoteToLocal => right_target.clone(),
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
        MergeDirection::LocalToRemote => {
            if !state.is_connected {
                state.status_message = "SSH not connected: cannot merge symlink".to_string();
                return;
            }
            match runtime.create_remote_symlink(&state.server_name, path, &source_target) {
                Ok(()) => {
                    state.status_message = format!(
                        "{}: symlink local -> {} merged (-> {})",
                        path, state.server_name, source_target
                    );
                }
                Err(e) => {
                    state.status_message = format!("Symlink merge failed: {}", e);
                }
            }
        }
        MergeDirection::RemoteToLocal => {
            match create_local_symlink(&state.local_tree.root, path, &source_target) {
                Ok(()) => {
                    state.status_message = format!(
                        "{}: symlink {} -> local merged (-> {})",
                        path, state.server_name, source_target
                    );
                }
                Err(e) => {
                    state.status_message = format!("Symlink merge failed: {}", e);
                }
            }
        }
    }
}

/// ローカルにシンボリックリンクを作成/更新する
fn create_local_symlink(
    root: &std::path::Path,
    rel_path: &str,
    target: &str,
) -> anyhow::Result<()> {
    let full_path = root.join(rel_path);

    // 既存のファイル/リンクを削除（TOCTOU 回避: 直接 remove して NotFound は無視）
    match std::fs::remove_file(&full_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    // シンボリックリンクを作成
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, &full_path)?;
    }

    #[cfg(not(unix))]
    {
        anyhow::bail!("Symlink creation is only supported on Unix systems");
    }

    Ok(())
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
}
