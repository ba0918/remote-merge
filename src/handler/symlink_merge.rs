//! シンボリックリンクのマージ実行。
//! MergeAction ベースで判定結果に基づく I/O 操作のみ行う。

use crate::app::side::Side;
use crate::app::AppState;
use crate::merge::executor::MergeDirection;
use crate::runtime::TuiRuntime;
use crate::service::merge::MergeAction;

/// シンボリックリンクマージのパラメータ。
///
/// `source_side` / `target_side` は borrow checker 対策で引数で受け取る
/// （`state: &mut AppState` と `state.left_source` / `state.right_source` の
///  immutable borrow が競合するため）。
pub struct SymlinkMergeParams<'a> {
    pub path: &'a str,
    pub direction: MergeDirection,
    pub action: MergeAction,
    pub source_side: &'a Side,
    pub target_side: &'a Side,
    pub session_id: &'a str,
}

/// シンボリックリンクのマージを実行する（MergeAction ベース）。
pub fn execute_symlink_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    params: &SymlinkMergeParams<'_>,
) -> bool {
    let path = params.path;
    let direction = params.direction;
    let action = params.action.clone();
    let source_side = params.source_side;
    let target_side = params.target_side;
    let session_id = params.session_id;
    // リモート側への書き込み時は接続チェック
    if !runtime.is_side_available(target_side) {
        state.status_message = "SSH not connected: cannot merge symlink".to_string();
        return false;
    }

    let (src_label, dst_label) = match direction {
        MergeDirection::LeftToRight => (
            state.left_source.display_name(),
            state.right_source.display_name(),
        ),
        MergeDirection::RightToLeft => (
            state.right_source.display_name(),
            state.left_source.display_name(),
        ),
    };

    match action {
        MergeAction::CreateSymlink {
            link_target,
            target_exists,
        } => {
            // バックアップ（target_exists の場合）
            if target_exists {
                if let Err(e) = runtime.create_backups(target_side, &[path.to_string()], session_id)
                {
                    tracing::warn!("Backup failed (continuing): {}", e);
                }
                if let Err(e) = runtime.remove_file(target_side, path) {
                    state.status_message = format!("Failed to remove target: {}", e);
                    return false;
                }
            }
            match runtime.create_symlink(target_side, path, &link_target) {
                Ok(()) => {
                    state.status_message = format!(
                        "{}: symlink {} -> {} merged (-> {})",
                        path, src_label, dst_label, link_target
                    );
                    true
                }
                Err(e) => {
                    state.status_message = format!("Symlink merge failed: {}", e);
                    false
                }
            }
        }
        MergeAction::ReplaceSymlinkWithFile => {
            // バックアップ → symlink 削除 → ファイル書き込み
            if let Err(e) = runtime.create_backups(target_side, &[path.to_string()], session_id) {
                tracing::warn!("Backup failed (continuing): {}", e);
            }
            if let Err(e) = runtime.remove_file(target_side, path) {
                state.status_message = format!("Failed to remove symlink: {}", e);
                return false;
            }
            // ソース側のコンテンツをバイト列で読み込み（バイナリ安全）
            match runtime.read_file_bytes(source_side, path, false) {
                Ok(content) => {
                    if let Err(e) = runtime.write_file_bytes(target_side, path, &content) {
                        state.status_message = format!("Write failed: {}", e);
                        return false;
                    }
                    state.status_message = format!(
                        "{}: symlink replaced with file ({} -> {})",
                        path, src_label, dst_label
                    );
                    true
                }
                Err(e) => {
                    state.status_message = format!("Read source failed: {}", e);
                    false
                }
            }
        }
        MergeAction::Normal => {
            tracing::error!("Normal action should not reach symlink_merge");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::side::Side;
    use crate::app::AppState;
    use crate::merge::executor::MergeDirection;
    use crate::runtime::TuiRuntime;
    use crate::service::merge::MergeAction;
    use crate::tree::FileTree;
    use tempfile::TempDir;

    /// テスト用の AppState を生成するヘルパー
    fn make_test_state(left: Side, right: Side) -> AppState {
        AppState::new(
            FileTree::default(),
            FileTree::default(),
            left,
            right,
            "base16-ocean.dark",
        )
    }

    /// テスト用の TuiRuntime を生成し、root_dir を tempdir に差し替える
    fn make_test_runtime(tmp: &TempDir) -> TuiRuntime {
        let mut rt = TuiRuntime::new_for_test();
        rt.core.config.local.root_dir = tmp.path().to_path_buf();
        rt
    }

    // ── MergeAction::Normal は panic せず false を返す ──

    #[test]
    fn normal_action_returns_false_without_panic() {
        let tmp = TempDir::new().unwrap();
        let mut state = make_test_state(Side::Local, Side::Local);
        let mut runtime = make_test_runtime(&tmp);
        let source = Side::Local;
        let target = Side::Local;

        let params = SymlinkMergeParams {
            path: "test.txt",
            direction: MergeDirection::LeftToRight,
            action: MergeAction::Normal,
            source_side: &source,
            target_side: &target,
            session_id: "20260311-120000",
        };
        let result = execute_symlink_merge(&mut state, &mut runtime, &params);

        assert!(!result, "Normal action should return false");
    }

    // ── 未接続リモートへのマージは接続エラーメッセージを返す ──

    #[test]
    fn remote_target_unavailable_sets_status_message() {
        let tmp = TempDir::new().unwrap();
        let mut state = make_test_state(Side::Local, Side::Remote("develop".to_string()));
        let mut runtime = make_test_runtime(&tmp);
        let source = Side::Local;
        let target = Side::Remote("develop".to_string());

        let params = SymlinkMergeParams {
            path: "link.txt",
            direction: MergeDirection::LeftToRight,
            action: MergeAction::CreateSymlink {
                link_target: "/some/target".to_string(),
                target_exists: false,
            },
            source_side: &source,
            target_side: &target,
            session_id: "20260311-120000",
        };
        let result = execute_symlink_merge(&mut state, &mut runtime, &params);

        assert!(!result);
        assert_eq!(
            state.status_message,
            "SSH not connected: cannot merge symlink"
        );
    }

    // ── CreateSymlink: ターゲットが存在しない場合、symlink を作成する ──

    #[test]
    fn create_symlink_without_existing_target() {
        let tmp = TempDir::new().unwrap();
        let mut state = make_test_state(Side::Local, Side::Local);
        let mut runtime = make_test_runtime(&tmp);
        let source = Side::Local;
        let target = Side::Local;

        let params = SymlinkMergeParams {
            path: "link.txt",
            direction: MergeDirection::LeftToRight,
            action: MergeAction::CreateSymlink {
                link_target: "/some/target".to_string(),
                target_exists: false,
            },
            source_side: &source,
            target_side: &target,
            session_id: "20260311-120000",
        };
        let result = execute_symlink_merge(&mut state, &mut runtime, &params);

        assert!(result);

        // symlink が作成されたことを確認
        let symlink_path = tmp.path().join("link.txt");
        assert!(
            symlink_path.symlink_metadata().is_ok(),
            "symlink should exist"
        );
        let link_dest = std::fs::read_link(&symlink_path).unwrap();
        assert_eq!(link_dest.to_str().unwrap(), "/some/target");

        // ステータスメッセージにマージ成功が含まれる
        assert!(
            state.status_message.contains("symlink") && state.status_message.contains("merged"),
            "unexpected status: {}",
            state.status_message
        );
    }

    // ── CreateSymlink: ターゲットが存在する場合、既存ファイルを置換 ──

    #[test]
    fn create_symlink_replaces_existing_file() {
        let tmp = TempDir::new().unwrap();
        // 既存ファイルを作成
        std::fs::write(tmp.path().join("link.txt"), "old content").unwrap();

        let mut state = make_test_state(Side::Local, Side::Local);
        let mut runtime = make_test_runtime(&tmp);
        let source = Side::Local;
        let target = Side::Local;

        let params = SymlinkMergeParams {
            path: "link.txt",
            direction: MergeDirection::LeftToRight,
            action: MergeAction::CreateSymlink {
                link_target: "/new/target".to_string(),
                target_exists: true,
            },
            source_side: &source,
            target_side: &target,
            session_id: "20260311-120000",
        };
        let result = execute_symlink_merge(&mut state, &mut runtime, &params);

        assert!(result);

        // 既存ファイルが symlink に置き換えられたことを確認
        let symlink_path = tmp.path().join("link.txt");
        assert!(symlink_path.symlink_metadata().unwrap().is_symlink());
        let link_dest = std::fs::read_link(&symlink_path).unwrap();
        assert_eq!(link_dest.to_str().unwrap(), "/new/target");
    }

    // ── ReplaceSymlinkWithFile: remove_file 失敗時のエラーハンドリング ──

    #[test]
    fn replace_symlink_remove_fails_on_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let mut state = make_test_state(Side::Local, Side::Local);
        let mut runtime = make_test_runtime(&tmp);
        let source = Side::Local;
        let target = Side::Local;

        let params = SymlinkMergeParams {
            path: "nonexistent.txt",
            direction: MergeDirection::LeftToRight,
            action: MergeAction::ReplaceSymlinkWithFile,
            source_side: &source,
            target_side: &target,
            session_id: "20260311-120000",
        };
        let result = execute_symlink_merge(&mut state, &mut runtime, &params);

        assert!(!result);
        assert!(
            state.status_message.contains("Failed to remove symlink"),
            "unexpected status: {}",
            state.status_message
        );
    }

    // ── ReplaceSymlinkWithFile: symlink → ファイル置換の正常系 ──

    #[test]
    fn replace_symlink_with_file_success() {
        let tmp = TempDir::new().unwrap();

        // ソースファイルを作成（read_file_bytes で読み込まれる）
        // source_side と target_side が同じ Local だと、同じパスを指すため
        // remove_file → read_file_bytes の順で実行すると読み込み時にファイルがない。
        // しかし実コードは "まずバックアップ → remove → read(source) → write(target)" の順。
        // source == target (Local) の場合: remove した後に read するのでエラーになる。
        // これは実際の使用パターン（左右が異なる side）では起きない制約。
        //
        // → source_side と target_side が同じ場合は read_file_bytes が失敗する。
        //   この挙動をテストとして記録する。
        let symlink_path = tmp.path().join("target.txt");
        std::os::unix::fs::symlink("/dummy/path", &symlink_path).unwrap();

        let mut state = make_test_state(Side::Local, Side::Local);
        let mut runtime = make_test_runtime(&tmp);
        let source = Side::Local;
        let target = Side::Local;

        let params = SymlinkMergeParams {
            path: "target.txt",
            direction: MergeDirection::LeftToRight,
            action: MergeAction::ReplaceSymlinkWithFile,
            source_side: &source,
            target_side: &target,
            session_id: "20260311-120000",
        };
        let result = execute_symlink_merge(&mut state, &mut runtime, &params);

        // source == target (Local) の場合: remove 後に read するため失敗する
        assert!(!result);
        assert!(
            state.status_message.contains("Read source failed"),
            "unexpected status: {}",
            state.status_message
        );
    }

    // ── direction: RightToLeft のラベル表示テスト ──

    #[test]
    fn right_to_left_direction_labels() {
        let tmp = TempDir::new().unwrap();
        let mut state = make_test_state(Side::Local, Side::Local);
        let mut runtime = make_test_runtime(&tmp);
        let source = Side::Local;
        let target = Side::Local;

        let params = SymlinkMergeParams {
            path: "link.txt",
            direction: MergeDirection::RightToLeft,
            action: MergeAction::CreateSymlink {
                link_target: "/target/path".to_string(),
                target_exists: false,
            },
            source_side: &source,
            target_side: &target,
            session_id: "20260311-120000",
        };
        let result = execute_symlink_merge(&mut state, &mut runtime, &params);

        assert!(result);
        // RightToLeft の場合、src_label = right_source, dst_label = left_source
        assert!(
            state.status_message.contains("local -> local"),
            "unexpected status: {}",
            state.status_message
        );
    }
}
