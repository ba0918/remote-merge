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
                        .left_tree
                        .find_node(std::path::Path::new(&path))
                        .is_some_and(|n| n.is_dir() && !n.is_loaded());
                    let remote_needs_load = state
                        .right_tree
                        .find_node(std::path::Path::new(&path))
                        .is_some_and(|n| n.is_dir() && !n.is_loaded());

                    if local_needs_load {
                        if state.left_source.is_local() {
                            state.load_local_children(&path);
                        } else {
                            let left_source = state.left_source.clone();
                            super::merge_exec::load_children_to(
                                state,
                                runtime,
                                &path,
                                &left_source,
                                true,
                            );
                        }
                    }
                    if remote_needs_load && runtime.is_side_available(&state.right_source) {
                        let right_source = state.right_source.clone();
                        super::merge_exec::load_children_to(
                            state,
                            runtime,
                            &path,
                            &right_source,
                            false,
                        );
                    }
                    // ref_tree も同期ロード（3way マージ整合性維持）
                    super::merge_tree_load::load_ref_children(state, runtime, &path);
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
                // 展開中のディレクトリ → 折りたたむ
                state.toggle_expand();
            } else {
                // ファイル or 閉じたディレクトリ → 親ディレクトリに移動して折りたたむ
                state.goto_parent_dir();
            }
        }
        KeyCode::Char('r') => {
            if !runtime.is_side_available(&state.right_source) {
                // 未接続時は再接続
                execute_reconnect(state, runtime);
            } else if state.current_is_dir() {
                if let Some(path) = state.current_path() {
                    state.refresh_directory(&path);
                }
            } else {
                // ファイル選択時は再接続
                execute_reconnect(state, runtime);
            }
            state.clear_scan_cache();
        }
        KeyCode::Char('f') => state.show_filter_panel(),
        KeyCode::Char('s') => state.show_pair_server_menu(),
        KeyCode::Char('c') => handle_clipboard_copy(state),
        KeyCode::Char('?') => state.show_help(),
        KeyCode::Char('F') => scanner::handle_diff_filter_toggle(state, runtime),
        KeyCode::Char('L') => handle_tree_merge(state, runtime, MergeDirection::RightToLeft),
        KeyCode::Char('R') => handle_tree_merge(state, runtime, MergeDirection::LeftToRight),
        KeyCode::Char('E') => handle_export_report(state),
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
        KeyCode::Char('W') => {
            super::three_way_summary_handler::open_three_way_summary(state);
        }
        KeyCode::Char('X') => {
            if state.has_reference() {
                super::reconnect::execute_ref_swap(state, runtime);
            } else {
                state.status_message = "No reference server".to_string();
            }
        }
        _ => {}
    }
}

/// ツリーマージ操作 (L/R キー)
///
/// ディレクトリ選択時: 展開済みファイル数が少なければ同期処理、
/// 多ければ非ブロッキング走査に切り替える。
/// ファイル選択時はそのまま表示。
fn handle_tree_merge(state: &mut AppState, runtime: &mut TuiRuntime, direction: MergeDirection) {
    // showing_ref_diff 中はマージ不可
    if state.showing_ref_diff {
        state.status_message = "Read-only: ref diff. Press X to swap and merge".to_string();
        return;
    }

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
    let has_unloaded = has_unloaded_children(&state.left_tree, dir_path)
        || has_unloaded_children(&state.right_tree, dir_path);

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

/// c キー: 選択中ファイルの diff をクリップボードにコピー
pub fn handle_clipboard_copy(state: &mut AppState) {
    use crate::app::clipboard::{format_diff_for_clipboard, ClipboardContext};
    use crate::service::status::is_sensitive;

    let path = match &state.selected_path {
        Some(p) => p.clone(),
        None => {
            state.status_message = "No file selected".to_string();
            return;
        }
    };

    // センシティブファイルチェック
    if is_sensitive(&path, &state.sensitive_patterns) {
        state.status_message = "Warning: sensitive file — content not copied".to_string();
        return;
    }

    let diff = match &state.current_diff {
        Some(d) => d,
        None => {
            state.status_message = "No diff available for this file".to_string();
            return;
        }
    };

    let left_label = state.left_source.display_name().to_string();
    let right_label = state.right_source.display_name().to_string();
    let left_root = state.left_tree.root.to_string_lossy().to_string();
    let right_root = state.right_tree.root.to_string_lossy().to_string();

    let context = ClipboardContext {
        file_path: path,
        left_label,
        right_label,
        left_root,
        right_root,
    };

    let text = format_diff_for_clipboard(&context, diff);

    match crate::app::clipboard_write::write_to_clipboard(&text) {
        crate::app::clipboard_write::ClipboardResult::Ok => {
            state.status_message = "Diff copied to clipboard".to_string();
        }
        crate::app::clipboard_write::ClipboardResult::WriteError(msg)
        | crate::app::clipboard_write::ClipboardResult::Unavailable(msg) => {
            state.status_message = msg;
        }
    }
}

/// Shift+E: レポート出力
fn handle_export_report(state: &mut AppState) {
    use crate::app::report::{generate_report, report_filename, ReportFileEntry, ReportInput};
    use crate::diff::engine::compute_diff;
    use std::collections::BTreeSet;
    use std::io::Write;

    // 左右のキャッシュからキーを収集（ソートして一貫した順序に）
    let keys: BTreeSet<String> = state
        .left_cache
        .keys()
        .chain(state.right_cache.keys())
        .cloned()
        .collect();

    if keys.is_empty() {
        state.status_message = "No cached files to export".to_string();
        return;
    }

    // キャッシュ済みの diff を計算
    let mut diffs: Vec<crate::diff::engine::DiffResult> = Vec::new();
    let mut entries: Vec<ReportFileEntry> = Vec::new();

    for path in &keys {
        let left = state.left_cache.get(path).map(|s| s.as_str());
        let right = state.right_cache.get(path).map(|s| s.as_str());

        let diff = match (left, right) {
            (Some(l), Some(r)) => compute_diff(l, r),
            _ => crate::diff::engine::DiffResult::Equal,
        };
        diffs.push(diff);
    }

    // ReportFileEntry は diff への参照を持つので、diffs を先に作ってからイテレート
    for (i, path) in keys.iter().enumerate() {
        entries.push(ReportFileEntry {
            path,
            left_content: state.left_cache.get(path).map(|s| s.as_str()),
            right_content: state.right_cache.get(path).map(|s| s.as_str()),
            diff: Some(&diffs[i]),
        });
    }

    let left_label = state.left_source.display_name();
    let right_label = state.right_source.display_name();
    let left_root = state.left_tree.root.to_string_lossy().to_string();
    let right_root = state.right_tree.root.to_string_lossy().to_string();

    let exclude = state.active_exclude_patterns();
    let input = ReportInput {
        left_label,
        right_label,
        left_root: &left_root,
        right_root: &right_root,
        sensitive_patterns: &state.sensitive_patterns,
        exclude_patterns: &exclude,
        files: entries,
    };

    let report = generate_report(&input);
    let filename = report_filename();

    match std::fs::File::create(&filename) {
        Ok(mut file) => match file.write_all(report.as_bytes()) {
            Ok(()) => {
                state.status_message = format!("Report exported: {}", filename);
            }
            Err(e) => {
                state.status_message = format!("Failed to write report: {}", e);
            }
        },
        Err(e) => {
            state.status_message = format!("Failed to create report file: {}", e);
        }
    }
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

    // -------------------------------------------------------
    // check_node_unloaded / has_unloaded_children 追加テスト
    // -------------------------------------------------------

    #[test]
    fn test_check_node_unloaded_deeply_nested_3_levels() {
        // 3階層: src/app/sub/deep — deep は未ロード
        let deep = FileNode::new_dir("deep");
        let sub = FileNode::new_dir_with_children("sub", vec![deep]);
        let app = FileNode::new_dir_with_children("app", vec![sub]);
        let tree = make_tree(vec![FileNode::new_dir_with_children("src", vec![app])]);
        assert!(has_unloaded_children(&tree, "src"));
    }

    #[test]
    fn test_check_node_unloaded_deeply_nested_all_loaded() {
        // 3階層すべてロード済み
        let deep = FileNode::new_dir_with_children("deep", vec![FileNode::new_file("leaf.rs")]);
        let sub = FileNode::new_dir_with_children("sub", vec![deep]);
        let app = FileNode::new_dir_with_children("app", vec![sub]);
        let tree = make_tree(vec![FileNode::new_dir_with_children("src", vec![app])]);
        assert!(!has_unloaded_children(&tree, "src"));
    }

    // -------------------------------------------------------
    // count_subtree_files テスト
    // -------------------------------------------------------

    use crate::app::side::Side;
    use crate::app::types::{Badge, FlatNode};
    use crate::app::AppState;

    fn make_test_state() -> AppState {
        AppState::new(
            make_tree(vec![]),
            make_tree(vec![]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        )
    }

    fn make_flat_file(path: &str) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: path.matches('/').count(),
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
            ref_only: false,
        }
    }

    fn make_flat_dir(path: &str, expanded: bool) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: path.matches('/').count(),
            is_dir: true,
            is_symlink: false,
            expanded,
            badge: Badge::Unchecked,
            ref_only: false,
        }
    }

    #[test]
    fn test_count_subtree_files_empty_flat_nodes() {
        let state = make_test_state();
        // flat_nodes が空 → (0, false)
        let (count, has_unloaded) = count_subtree_files(&state, "src");
        assert_eq!(count, 0);
        assert!(!has_unloaded);
    }

    #[test]
    fn test_count_subtree_files_single_file() {
        let mut state = make_test_state();
        state.flat_nodes = vec![make_flat_dir("src", true), make_flat_file("src/main.rs")];
        let (count, has_unloaded) = count_subtree_files(&state, "src");
        assert_eq!(count, 1);
        assert!(!has_unloaded);
    }

    #[test]
    fn test_count_subtree_files_multiple_files_in_dir() {
        let mut state = make_test_state();
        state.flat_nodes = vec![
            make_flat_dir("src", true),
            make_flat_file("src/main.rs"),
            make_flat_file("src/lib.rs"),
            make_flat_file("src/util.rs"),
        ];
        let (count, has_unloaded) = count_subtree_files(&state, "src");
        assert_eq!(count, 3);
        assert!(!has_unloaded);
    }

    #[test]
    fn test_count_subtree_files_nested_dirs() {
        let mut state = make_test_state();
        state.flat_nodes = vec![
            make_flat_dir("src", true),
            make_flat_file("src/main.rs"),
            make_flat_dir("src/app", true),
            make_flat_file("src/app/mod.rs"),
            make_flat_file("src/app/state.rs"),
            make_flat_dir("src/app/sub", true),
            make_flat_file("src/app/sub/deep.rs"),
        ];
        let (count, has_unloaded) = count_subtree_files(&state, "src");
        // main.rs + mod.rs + state.rs + deep.rs = 4
        assert_eq!(count, 4);
        assert!(!has_unloaded);
    }

    #[test]
    fn test_count_subtree_files_with_unloaded_subdir() {
        let mut state = make_test_state();
        // left_tree に未ロードのサブディレクトリを設定
        state.left_tree = make_tree(vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_dir("app")], // app は children=None（未ロード）
        )]);
        state.flat_nodes = vec![make_flat_dir("src", true), make_flat_file("src/main.rs")];
        let (count, has_unloaded) = count_subtree_files(&state, "src");
        assert_eq!(count, 1);
        assert!(has_unloaded);
    }

    #[test]
    fn test_count_subtree_files_nonexistent_path() {
        let mut state = make_test_state();
        state.flat_nodes = vec![make_flat_dir("src", true), make_flat_file("src/main.rs")];
        // 存在しないパス → 配下ファイルなし
        let (count, has_unloaded) = count_subtree_files(&state, "nonexistent");
        assert_eq!(count, 0);
        assert!(!has_unloaded);
    }

    #[test]
    fn test_count_subtree_files_does_not_count_dirs() {
        let mut state = make_test_state();
        state.flat_nodes = vec![
            make_flat_dir("src", true),
            make_flat_dir("src/app", true),
            make_flat_dir("src/handler", true),
        ];
        // ディレクトリのみ → ファイル数 0
        let (count, _) = count_subtree_files(&state, "src");
        assert_eq!(count, 0);
    }

    // -------------------------------------------------------
    // AppState メソッド単体テスト
    // NOTE: handle_tree_key() は TuiRuntime を要求するため直接呼べない。
    // 以下は handle_tree_key から呼び出される AppState メソッドの動作テスト。
    // -------------------------------------------------------

    #[test]
    fn test_toggle_focus() {
        let mut state = make_test_state();
        assert_eq!(state.focus, crate::app::types::Focus::FileTree);
        state.toggle_focus();
        assert_eq!(state.focus, crate::app::types::Focus::DiffView);
        state.toggle_focus();
        assert_eq!(state.focus, crate::app::types::Focus::FileTree);
    }

    #[test]
    fn test_cursor_down_and_up() {
        let mut state = make_test_state();
        state.flat_nodes = vec![
            make_flat_dir("src", true),
            make_flat_file("src/a.rs"),
            make_flat_file("src/b.rs"),
        ];
        assert_eq!(state.tree_cursor, 0);
        state.cursor_down();
        assert_eq!(state.tree_cursor, 1);
        state.cursor_down();
        assert_eq!(state.tree_cursor, 2);
        // 末尾を超えない
        state.cursor_down();
        assert_eq!(state.tree_cursor, 2);
        state.cursor_up();
        assert_eq!(state.tree_cursor, 1);
        state.cursor_up();
        assert_eq!(state.tree_cursor, 0);
        // 先頭を超えない
        state.cursor_up();
        assert_eq!(state.tree_cursor, 0);
    }

    #[test]
    fn test_cycle_theme() {
        let mut state = make_test_state();
        let initial_theme = state.theme_name.clone();
        state.cycle_theme();
        // テーマが変わったことを確認（次のテーマに遷移）
        assert_ne!(state.theme_name, initial_theme);
    }

    #[test]
    fn test_toggle_syntax_highlight() {
        let mut state = make_test_state();
        let initial = state.syntax_highlight_enabled;
        state.toggle_syntax_highlight();
        assert_ne!(state.syntax_highlight_enabled, initial);
        state.toggle_syntax_highlight();
        assert_eq!(state.syntax_highlight_enabled, initial);
    }

    #[test]
    fn test_show_help() {
        let mut state = make_test_state();
        assert!(matches!(state.dialog, crate::ui::dialog::DialogState::None));
        state.show_help();
        assert!(matches!(
            state.dialog,
            crate::ui::dialog::DialogState::Help(_)
        ));
    }

    #[test]
    fn test_toggle_diff_filter() {
        let mut state = make_test_state();
        assert!(!state.diff_filter_mode);
        state.toggle_diff_filter();
        assert!(state.diff_filter_mode);
        state.toggle_diff_filter();
        assert!(!state.diff_filter_mode);
    }

    #[test]
    fn test_toggle_diff_mode() {
        let mut state = make_test_state();
        assert_eq!(state.diff_mode, crate::app::types::DiffMode::Unified);
        state.toggle_diff_mode();
        assert_eq!(state.diff_mode, crate::app::types::DiffMode::SideBySide);
        state.toggle_diff_mode();
        assert_eq!(state.diff_mode, crate::app::types::DiffMode::Unified);
    }

    // -------------------------------------------------------
    // handle_clipboard_copy ロジック分岐テスト
    // -------------------------------------------------------

    #[test]
    fn test_clipboard_copy_no_file_selected() {
        let mut state = make_test_state();
        state.selected_path = None;
        handle_clipboard_copy(&mut state);
        assert_eq!(state.status_message, "No file selected");
    }

    #[test]
    fn test_clipboard_copy_sensitive_file() {
        let mut state = make_test_state();
        state.selected_path = Some(".env".to_string());
        state.sensitive_patterns = vec![".env".to_string()];
        handle_clipboard_copy(&mut state);
        assert_eq!(
            state.status_message,
            "Warning: sensitive file — content not copied"
        );
    }

    #[test]
    fn test_clipboard_copy_no_diff_available() {
        let mut state = make_test_state();
        state.selected_path = Some("src/main.rs".to_string());
        state.current_diff = None;
        handle_clipboard_copy(&mut state);
        assert_eq!(state.status_message, "No diff available for this file");
    }
}
