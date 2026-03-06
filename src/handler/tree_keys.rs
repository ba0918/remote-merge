//! FileTree フォーカス時のキー処理。

use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{AppState, MergeScanState, ScanState};
use crate::merge::executor::MergeDirection;
use crate::runtime::TuiRuntime;
use crate::runtime::{merge_scan, scanner};

use super::merge_exec::{expand_subtree_for_merge, load_file_content, load_subtree_contents};
use super::reconnect::execute_reconnect;

/// 非同期走査に切り替える閾値（この数以下なら同期処理）
const SYNC_FILE_THRESHOLD: usize = 20;

/// FileTree フォーカス時のキーハンドリング
pub fn handle_tree_key(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    code: KeyCode,
    _modifiers: KeyModifiers,
) {
    // 検索モード中は search_keys にディスパッチ
    if state.search_state.active {
        super::search_keys::handle_search_key(state, code);
        return;
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            if !matches!(state.merge_scan_state, MergeScanState::Idle) && code == KeyCode::Esc {
                state.merge_scan_state = MergeScanState::Idle;
                runtime.merge_scan_receiver = None;
                state.dialog = crate::ui::dialog::DialogState::None;
                state.status_message = "Merge scan cancelled".to_string();
            } else if matches!(state.scan_state, ScanState::Scanning) && code == KeyCode::Esc {
                state.scan_state = ScanState::Idle;
                runtime.scan_receiver = None;
                state.status_message = "Scan cancelled".to_string();
            } else if state.search_state.has_query() && code == KeyCode::Esc {
                // 検索結果がある場合は Esc で検索クリア（quit しない）
                state.search_state.clear();
                state.rebuild_flat_nodes();
                state.status_message = String::new();
            } else {
                state.should_quit = true;
            }
        }
        KeyCode::Tab => state.toggle_focus(),
        KeyCode::Up | KeyCode::Char('k') => state.cursor_up(),
        KeyCode::Down | KeyCode::Char('j') => state.cursor_down(),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
            if state
                .flat_nodes
                .get(state.tree_cursor)
                .is_some_and(|n| n.is_dir)
            {
                if let Some(path) = state.current_path() {
                    let local_needs_load = state
                        .local_tree
                        .find_node(std::path::Path::new(&path))
                        .is_some_and(|n| n.is_dir() && !n.is_loaded());
                    let remote_needs_load = state
                        .remote_tree
                        .find_node(std::path::Path::new(&path))
                        .is_some_and(|n| n.is_dir() && !n.is_loaded());

                    if local_needs_load {
                        state.load_local_children(&path);
                    }
                    if remote_needs_load && state.is_connected {
                        super::merge_exec::load_remote_children(state, runtime, &path);
                    }
                }
                state.toggle_expand();
            } else {
                load_file_content(state, runtime);
                state.select_file();
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if state
                .flat_nodes
                .get(state.tree_cursor)
                .is_some_and(|n| n.is_dir && n.expanded)
            {
                state.toggle_expand();
            }
        }
        KeyCode::Char('r') => {
            if state.current_is_dir() {
                if let Some(path) = state.current_path() {
                    state.refresh_directory(&path);
                }
            } else {
                state.clear_cache();
            }
            state.clear_scan_cache();
        }
        KeyCode::Char('f') => state.show_filter_panel(),
        KeyCode::Char('s') => state.show_server_menu(),
        KeyCode::Char('c') => execute_reconnect(state, runtime),
        KeyCode::Char('?') => state.show_help(),
        KeyCode::Char('F') => scanner::handle_diff_filter_toggle(state, runtime),
        KeyCode::Char('L') => handle_tree_merge(state, runtime, MergeDirection::RemoteToLocal),
        KeyCode::Char('R') => handle_tree_merge(state, runtime, MergeDirection::LocalToRemote),
        KeyCode::Char('T') => state.cycle_theme(),
        KeyCode::Char('S') => state.toggle_syntax_highlight(),
        KeyCode::Char('/') => {
            // 検索前にローカルツリーを全展開ロード（未取得ディレクトリをスキャン）
            state.load_local_tree_recursive();
            state.search_state.activate();
            state.status_message = "/".to_string();
        }
        KeyCode::Char('n') => super::search_keys::jump_next(state),
        KeyCode::Char('N') => super::search_keys::jump_prev(state),
        _ => {}
    }
}

/// ツリーマージ操作 (L/R キー)
///
/// ディレクトリ選択時: 展開済みファイル数が少なければ同期処理、
/// 多ければ非ブロッキング走査に切り替える。
/// ファイル選択時はそのまま表示。
fn handle_tree_merge(state: &mut AppState, runtime: &mut TuiRuntime, direction: MergeDirection) {
    // マージ走査中は無視
    if !matches!(state.merge_scan_state, MergeScanState::Idle) {
        state.status_message = "Merge scan in progress. Please wait or press Esc.".to_string();
        return;
    }

    let is_dir = state
        .flat_nodes
        .get(state.tree_cursor)
        .is_some_and(|n| n.is_dir);

    if is_dir {
        if let Some(path) = state.current_path() {
            // 展開済みファイル数と未ロードサブディレクトリの有無を判定
            let (file_count, has_unloaded) = count_subtree_files(state, &path);

            if file_count <= SYNC_FILE_THRESHOLD && !has_unloaded {
                // 同期処理（プログレスダイアログ表示）
                state.dialog = crate::ui::dialog::DialogState::Progress(
                    crate::ui::dialog::ProgressDialog::new(
                        crate::ui::dialog::ProgressPhase::Scanning,
                        path.as_str(),
                        false,
                    ),
                );
                expand_subtree_for_merge(state, runtime, &path);
                load_subtree_contents(state, runtime, &path);
                state.dialog = crate::ui::dialog::DialogState::None;
                state.show_merge_dialog(direction);
            } else {
                // 非同期走査に切り替え
                merge_scan::start_merge_scan(state, runtime, &path, direction);
            }
        }
    } else {
        // Unchecked ファイルはコンテンツを読み込んで差分を確定させてからダイアログ表示
        ensure_file_checked(state, runtime);
        state.show_merge_dialog(direction);
    }
}

/// ファイルが Unchecked なら、コンテンツをロードしてバッジを確定させる。
fn ensure_file_checked(state: &mut AppState, runtime: &mut TuiRuntime) {
    let badge = state.flat_nodes.get(state.tree_cursor).map(|n| n.badge);
    if badge == Some(crate::app::Badge::Unchecked) {
        load_file_content(state, runtime);
        state.select_file();
        state.rebuild_flat_nodes();
    }
}

/// ディレクトリ配下の展開済みファイル数と未ロードサブディレクトリの有無を返す
fn count_subtree_files(state: &AppState, dir_path: &str) -> (usize, bool) {
    let prefix = format!("{}/", dir_path);
    let mut file_count = 0;

    // flat_nodes に既にあるファイルをカウント
    for node in &state.flat_nodes {
        if node.path.starts_with(&prefix) && !node.is_dir {
            file_count += 1;
        }
    }

    // ローカル・リモートツリーで未ロードのサブディレクトリを検索
    let has_unloaded = has_unloaded_children(&state.local_tree, dir_path)
        || has_unloaded_children(&state.remote_tree, dir_path);

    (file_count, has_unloaded)
}

/// 指定パスのツリーノード配下に未ロードのサブディレクトリがあるか
fn has_unloaded_children(tree: &crate::tree::FileTree, dir_path: &str) -> bool {
    let node = match tree.find_node(std::path::Path::new(dir_path)) {
        Some(n) => n,
        None => return false,
    };
    if !node.is_dir() {
        return false;
    }
    check_node_unloaded(node)
}

/// ノード自体と全子孫の未ロード状態を再帰チェック
fn check_node_unloaded(node: &crate::tree::FileNode) -> bool {
    if !node.is_loaded() {
        return true;
    }
    if let Some(children) = &node.children {
        for child in children {
            if child.is_dir() && check_node_unloaded(child) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    #[test]
    fn test_has_unloaded_children_all_loaded() {
        let tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )]);
        assert!(!has_unloaded_children(&tree, "src"));
    }

    #[test]
    fn test_has_unloaded_children_unloaded_dir() {
        // children = None は未ロード
        let tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_dir("app")], // app は children=None（未ロード）
        )]);
        assert!(has_unloaded_children(&tree, "src"));
    }

    #[test]
    fn test_has_unloaded_children_nested_unloaded() {
        let app = FileNode::new_dir_with_children("app", vec![FileNode::new_dir("sub")]);
        // app は loaded だが、app/sub は未ロード (children=None)
        assert!(app.is_loaded());

        let tree = make_tree(vec![FileNode::new_dir_with_children("src", vec![app])]);
        assert!(has_unloaded_children(&tree, "src"));
    }

    #[test]
    fn test_has_unloaded_children_all_nested_loaded() {
        let sub = FileNode::new_dir_with_children("sub", vec![FileNode::new_file("c.rs")]);
        let app = FileNode::new_dir_with_children("app", vec![FileNode::new_file("a.rs"), sub]);
        let tree = make_tree(vec![FileNode::new_dir_with_children("src", vec![app])]);
        assert!(!has_unloaded_children(&tree, "src"));
    }

    #[test]
    fn test_has_unloaded_children_nonexistent_path() {
        let tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        // 存在しないパスは false
        assert!(!has_unloaded_children(&tree, "nonexistent"));
    }

    #[test]
    fn test_has_unloaded_children_file_path() {
        let tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )]);
        // ファイルパスは false
        assert!(!has_unloaded_children(&tree, "src/a.rs"));
    }

    #[test]
    fn test_check_node_unloaded_leaf_dir() {
        // children=None のディレクトリは未ロード
        let node = FileNode::new_dir("empty");
        assert!(check_node_unloaded(&node));
    }

    #[test]
    fn test_check_node_unloaded_loaded_empty_dir() {
        // children=Some([]) の空ディレクトリはロード済み
        let node = FileNode::new_dir_with_children("empty", vec![]);
        assert!(!check_node_unloaded(&node));
    }
}
