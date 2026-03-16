//! 接続管理（再接続・サーバ切替）。

use std::collections::HashSet;

use crate::app::side::comparison_label;
use crate::app::{AppState, Side};
use crate::filter;
use crate::runtime::TuiRuntime;

/// SSH 再接続を実行する（r キー）
///
/// left_source / right_source に応じて必要な接続を再確立する。
pub fn execute_reconnect(state: &mut AppState, runtime: &mut TuiRuntime) {
    let left_source = state.left_source.clone();
    let right_source = state.right_source.clone();
    let label = comparison_label(&left_source, &right_source);
    state.status_message = format!("Reconnecting ({})...", label);

    // 展開状態・カーソル位置・スクロール位置を保存
    let expanded_backup = state.expanded_dirs.clone();
    let cursor_path = state.current_path();
    let scroll_backup = state.tree_scroll;

    // 左側の再接続
    let left_ok = reconnect_side(state, runtime, &left_source, true);

    // 右側の再接続
    let right_ok = reconnect_side(state, runtime, &right_source, false);

    if !left_ok || !right_ok {
        return;
    }

    // 再接続時は全バッジスキャンをキャンセル
    crate::runtime::badge_scan::cancel_all_badge_scans(state, runtime);

    state.clear_all_content_caches();
    state.current_diff = None;
    state.selected_path = None;
    state.reset_diff_view_state();
    state.undo_stack.clear();
    state.is_connected = true;
    state.showing_ref_diff = false;
    state.clear_scan_cache();

    // Agent 接続状態を同期
    state.sync_agent_status(runtime.core.agent_clients.keys());

    // reference サーバのキャッシュ・ツリーをクリアして再取得
    if state.ref_source.is_some() {
        state.ref_tree = None;
        state.ref_cache.clear();
        state.conflict_cache.clear();
        execute_ref_connect(state, runtime);
    }

    // ツリー状態を復元
    restore_tree_state(
        state,
        runtime,
        expanded_backup,
        cursor_path,
        scroll_backup,
        &left_source,
        &right_source,
    );

    state.status_message = format!("Reconnected: {} | tree state restored", label);
}

/// 片側の再接続を実行する
///
/// `is_left` が true なら left_tree を更新、false なら right_tree を更新。
fn reconnect_side(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    source: &Side,
    is_left: bool,
) -> bool {
    match source {
        Side::Local => {
            // ローカルはツリーを再スキャンするだけ
            match crate::local::scan_local_tree(
                &runtime.core.config.local.root_dir,
                &runtime.core.config.filter.exclude,
            ) {
                Ok(mut tree) => {
                    filter::filter_tree_by_include(&mut tree, &runtime.core.config.filter.include);
                    if is_left {
                        state.left_tree = tree;
                    } else {
                        state.right_tree = tree;
                    }
                    true
                }
                Err(e) => {
                    state.status_message = format!("Local scan failed: {}", e);
                    false
                }
            }
        }
        Side::Remote(server_name) => {
            runtime.disconnect(server_name);
            match runtime.connect(server_name) {
                Ok(()) => match runtime.fetch_remote_tree(server_name) {
                    Ok(mut tree) => {
                        filter::filter_tree_by_include(
                            &mut tree,
                            &runtime.core.config.filter.include,
                        );
                        if is_left {
                            state.left_tree = tree;
                        } else {
                            state.right_tree = tree;
                        }
                        true
                    }
                    Err(e) => {
                        state.is_connected = false;
                        state.status_message = format!("{} tree fetch failed: {}", server_name, e);
                        false
                    }
                },
                Err(e) => {
                    state.is_connected = false;
                    state.status_message = format!(
                        "{} reconnection failed: {} | Press 'r' to retry",
                        server_name, e
                    );
                    false
                }
            }
        }
    }
}

/// カーソル位置を指定パスに最も近いノードに復元する
fn restore_cursor_position(state: &mut AppState, target_path: &str) {
    // 完全一致を試みる
    if let Some(idx) = state.flat_nodes.iter().position(|n| n.path == target_path) {
        state.tree_cursor = idx;
        return;
    }
    // 親ディレクトリにフォールバック
    let mut path = target_path.to_string();
    while let Some(pos) = path.rfind('/') {
        path.truncate(pos);
        if let Some(idx) = state.flat_nodes.iter().position(|n| n.path == path) {
            state.tree_cursor = idx;
            return;
        }
    }
}

/// ツリー状態（展開・カーソル・スクロール）を復元する共通ヘルパー。
fn restore_tree_state(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    expanded_backup: HashSet<String>,
    cursor_path: Option<String>,
    scroll_backup: usize,
    left_source: &Side,
    right_source: &Side,
) {
    // 検索状態をクリア
    state.search_state.clear();
    state.diff_search_state.clear();

    // 展開状態を復元（新ツリーに存在する or 未ロードのディレクトリのみ）
    // 浅いスキャンでは中間ディレクトリの children が None のため、
    // find_node() だと NotFound 扱いになってしまう。
    // find_node_or_unloaded() で 3値判定し、Unloaded も保持する。
    use crate::tree::NodePresence;
    state.expanded_dirs = expanded_backup
        .into_iter()
        .filter(|dir| {
            let path = std::path::Path::new(dir);
            let left_presence = state.left_tree.find_node_or_unloaded(path);
            let right_presence = state.right_tree.find_node_or_unloaded(path);
            // NotFound のみ除外。Found / Unloaded は保持。
            left_presence != NodePresence::NotFound || right_presence != NodePresence::NotFound
        })
        .collect();

    // 展開済みディレクトリの子ノードを深さ順に再取得
    // 親→子の順でロードしないと、親の children が未取得のため
    // 子の find_node_mut が None を返してスキップされてしまう
    let mut dirs: Vec<String> = state.expanded_dirs.iter().cloned().collect();
    dirs.sort_by_key(|d| d.matches('/').count());
    for dir in &dirs {
        super::merge_exec::load_children_to(state, runtime, dir, left_source, true);
        super::merge_exec::load_children_to(state, runtime, dir, right_source, false);
        super::merge_tree_load::load_ref_children(state, runtime, dir);
    }

    state.rebuild_flat_nodes();

    // スクロール位置を復元（範囲外にならないようクランプ）
    state.tree_scroll = scroll_backup.min(state.flat_nodes.len().saturating_sub(1));

    // カーソル位置を復元
    if let Some(path) = cursor_path {
        restore_cursor_position(state, &path);
    }

    // 展開済みディレクトリ＋ルート直下のバッジスキャンを自動起動
    start_badge_scans_for_visible_dirs(state, runtime);
}

/// right ↔ ref スワップを実行する（X キー）
///
/// unsaved changes がある場合は UnsavedChanges ダイアログを表示。
/// swap 後に ref_tree が None なら `execute_ref_connect()` で取得。
pub fn execute_ref_swap(state: &mut AppState, runtime: &mut TuiRuntime) {
    if state.has_unsaved_changes() {
        state.dialog = crate::ui::dialog::DialogState::UnsavedChanges;
        return;
    }

    // 旧 right のバッジスキャンをキャンセル（結果が新 right に誤適用されるのを防ぐ）
    crate::runtime::badge_scan::cancel_all_badge_scans(state, runtime);

    // 選択中のパスを保持（swap 後にカーソル位置と diff を再計算するため）
    let selected_path = state.selected_path.clone();

    state.swap_right_ref();

    // swap 後の新しい right サーバに Agent が未起動なら起動を試みる
    if let Side::Remote(ref server_name) = state.right_source {
        if runtime.core.get_agent(server_name).is_none() {
            let _ = runtime.core.try_start_agent(server_name);
        }
    }

    // Agent 接続状態を同期
    state.sync_agent_status(runtime.core.agent_clients.keys());

    // ref_tree が None なら接続して取得
    if state.ref_tree.is_none() && state.ref_source.is_some() {
        execute_ref_connect(state, runtime);
    }

    // swap 後に選択中のパスが flat_nodes に残っていればカーソル位置を復元
    if let Some(ref path) = selected_path {
        if let Some(idx) = state.flat_nodes.iter().position(|n| n.path == *path) {
            state.tree_cursor = idx;
            // コンテンツをロード（新しい right からの読み込みが必要な場合）
            super::merge_exec::load_file_content(state, runtime);
            state.select_file();
        } else {
            // パスが見つからない場合はカーソル位置をクランプ
            state.selected_path = None;
            state.current_diff = None;
        }
    }

    // 展開済みディレクトリ＋ルート直下のバッジスキャンを自動起動
    start_badge_scans_for_visible_dirs(state, runtime);
}

/// ペアサーバ切替を実行する（LEFT/RIGHT 両方を変更）
pub fn execute_pair_switch(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    left_name: &str,
    right_name: &str,
) {
    state.status_message = format!("Switching to {} <-> {}...", left_name, right_name);

    // 切替前にツリー状態をバックアップ
    let expanded_backup = state.expanded_dirs.clone();
    let cursor_path = state.current_path();
    let scroll_backup = state.tree_scroll;

    // 旧サーバのバッジスキャンをキャンセル（結果が新サーバに適用されるのを防ぐ）
    crate::runtime::badge_scan::cancel_all_badge_scans(state, runtime);

    // Side を構築
    let new_left = Side::new(left_name);
    let new_right = Side::new(right_name);

    // 左側ツリーを取得
    let mut left_tree = match &new_left {
        Side::Local => {
            match crate::local::scan_local_tree(
                &runtime.core.config.local.root_dir,
                &runtime.core.config.filter.exclude,
            ) {
                Ok(tree) => tree,
                Err(e) => {
                    state.status_message = format!("Local scan failed: {}", e);
                    return;
                }
            }
        }
        Side::Remote(name) => {
            if let Err(e) = runtime.connect(name) {
                state.status_message = format!("{} connection failed: {}", name, e);
                return;
            }
            match runtime.fetch_remote_tree(name) {
                Ok(tree) => tree,
                Err(e) => {
                    state.status_message = format!("{} tree fetch failed: {}", name, e);
                    return;
                }
            }
        }
    };

    // 右側ツリーを取得
    let mut right_tree = match &new_right {
        Side::Local => {
            match crate::local::scan_local_tree(
                &runtime.core.config.local.root_dir,
                &runtime.core.config.filter.exclude,
            ) {
                Ok(tree) => tree,
                Err(e) => {
                    state.status_message = format!("Local scan failed: {}", e);
                    return;
                }
            }
        }
        Side::Remote(name) => {
            if let Err(e) = runtime.connect(name) {
                state.status_message = format!("{} connection failed: {}", name, e);
                return;
            }
            match runtime.fetch_remote_tree(name) {
                Ok(tree) => tree,
                Err(e) => {
                    state.status_message = format!("{} tree fetch failed: {}", name, e);
                    return;
                }
            }
        }
    };

    // include フィルターを適用
    filter::filter_tree_by_include(&mut left_tree, &runtime.core.config.filter.include);
    filter::filter_tree_by_include(&mut right_tree, &runtime.core.config.filter.include);

    state.switch_pair(new_left, new_right, left_tree, right_tree);

    // Agent 接続状態を同期（connect() 内で Agent 起動済み）
    state.sync_agent_status(runtime.core.agent_clients.keys());

    // reference サーバが自動選択されたら接続 + ツリー取得
    // 注: left/right と同じ深さ（浅いスキャン）で取得する
    execute_ref_connect(state, runtime);

    // ツリー状態を復元
    let left_source = state.left_source.clone();
    let right_source = state.right_source.clone();
    restore_tree_state(
        state,
        runtime,
        expanded_backup,
        cursor_path,
        scroll_backup,
        &left_source,
        &right_source,
    );

    let label = comparison_label(&state.left_source, &state.right_source);
    state.status_message = format!(
        "{} | tree state restored | Tab: switch focus | s: server | q: quit",
        label
    );
}

/// reference サーバに接続してツリーを取得する。
///
/// ref_source が Some かつ ref_tree が None の場合のみ実行。
/// 接続失敗時は graceful degradation（ref をクリア）。
pub fn execute_ref_connect(state: &mut AppState, runtime: &mut TuiRuntime) {
    let ref_source = match &state.ref_source {
        Some(source) if state.ref_tree.is_none() => source.clone(),
        _ => return,
    };

    // left/right と同じ深さ（浅いスキャン = ルート直下のみ）で取得する。
    // 再帰取得すると遅延ロードの left/right との深さ不一致で
    // 全ファイルが ref_only 判定されてしまうため。
    // ref のファイル内容は load_ref_file_content で個別に遅延取得される。
    let tree_acquired = match &ref_source {
        Side::Local => {
            match crate::local::scan_local_tree(
                &runtime.core.config.local.root_dir,
                &runtime.core.config.filter.exclude,
            ) {
                Ok(mut tree) => {
                    filter::filter_tree_by_include(&mut tree, &runtime.core.config.filter.include);
                    state.ref_tree = Some(tree);
                    true
                }
                Err(_) => {
                    state.clear_reference();
                    false
                }
            }
        }
        Side::Remote(name) => {
            if runtime.connect(name).is_err() {
                state.clear_reference();
                return;
            }
            match runtime.fetch_remote_tree(name) {
                Ok(mut tree) => {
                    filter::filter_tree_by_include(&mut tree, &runtime.core.config.filter.include);
                    state.ref_tree = Some(tree);
                    true
                }
                Err(_) => {
                    state.clear_reference();
                    false
                }
            }
        }
    };

    // ref_tree 取得後に flat_nodes を再構築（ref_only ノードを含めるため）
    if tree_acquired {
        state.rebuild_flat_nodes();
    }
}

/// 展開済みディレクトリ＋ルート直下のバッジスキャンをまとめて起動する。
///
/// expanded_dirs を事前にクローンしてからループするため、
/// start_badge_scan 内での state 変更と衝突しない。
fn start_badge_scans_for_visible_dirs(state: &mut AppState, runtime: &mut TuiRuntime) {
    let scan_dirs: Vec<String> = state.expanded_dirs.iter().cloned().collect();
    for dir in scan_dirs {
        crate::runtime::badge_scan::start_badge_scan(state, runtime, &dir);
    }
    // ルート直下は未展開でもトップレベルに表示されるため常にスキャン
    crate::runtime::badge_scan::start_badge_scan(state, runtime, "");
}

/// サーバ切替を実行する（右側のサーバを切り替え）
pub fn execute_server_switch(state: &mut AppState, runtime: &mut TuiRuntime, server_name: &str) {
    let new_side = Side::new(server_name);

    state.status_message = format!("Switching to {}...", server_name);

    // 切替前にツリー状態をバックアップ
    let expanded_backup = state.expanded_dirs.clone();
    let cursor_path = state.current_path();
    let scroll_backup = state.tree_scroll;

    // 旧サーバのバッジスキャンをキャンセル（結果が新サーバに適用されるのを防ぐ）
    crate::runtime::badge_scan::cancel_all_badge_scans(state, runtime);

    // 古い右側接続を切断
    if let Side::Remote(old_name) = &state.right_source {
        runtime.disconnect(old_name);
    }

    // Side に応じてツリーを取得
    let mut tree = match &new_side {
        Side::Local => {
            match crate::local::scan_local_tree(
                &runtime.core.config.local.root_dir,
                &runtime.core.config.filter.exclude,
            ) {
                Ok(tree) => tree,
                Err(e) => {
                    state.status_message = format!("Local scan failed: {}", e);
                    return;
                }
            }
        }
        Side::Remote(name) => match runtime.connect(name) {
            Ok(()) => match runtime.fetch_remote_tree(name) {
                Ok(tree) => tree,
                Err(e) => {
                    state.status_message = format!("{} tree fetch failed: {}", name, e);
                    return;
                }
            },
            Err(e) => {
                state.status_message = format!("{} connection failed: {}", name, e);
                return;
            }
        },
    };

    // include フィルターを適用
    filter::filter_tree_by_include(&mut tree, &runtime.core.config.filter.include);

    state.switch_server(new_side, tree);

    // Agent 接続状態を同期
    state.sync_agent_status(runtime.core.agent_clients.keys());

    // reference サーバを自動再選択 + ツリー取得
    // 注: left/right と同じ深さ（浅いスキャン）で取得する
    execute_ref_connect(state, runtime);

    // ツリー状態を復元
    let left_source = state.left_source.clone();
    let right_source = state.right_source.clone();
    restore_tree_state(
        state,
        runtime,
        expanded_backup,
        cursor_path,
        scroll_backup,
        &left_source,
        &right_source,
    );

    let label = comparison_label(&state.left_source, &state.right_source);
    state.status_message = format!(
        "{} | tree state restored | Tab: switch focus | s: server | q: quit",
        label
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, FlatNode};
    use crate::runtime::badge_scan::BadgeScanEntry;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::Arc;

    /// テスト用の FlatNode を作成するヘルパー
    fn make_flat_node(path: &str) -> FlatNode {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        FlatNode {
            path: path.to_string(),
            name,
            depth: path.matches('/').count(),
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
            ref_only: false,
        }
    }

    /// テスト用の AppState を作成するヘルパー
    fn make_state() -> AppState {
        let tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_file("a.txt")],
        };
        let tree2 = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_file("a.txt")],
        };
        AppState::new(
            tree,
            tree2,
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        )
    }

    #[test]
    fn restore_cursor_exact_match() {
        let mut state = make_state();
        state.flat_nodes = vec![
            make_flat_node("README.md"),
            make_flat_node("src/main.rs"),
            make_flat_node("src/lib.rs"),
        ];
        state.tree_cursor = 0;

        restore_cursor_position(&mut state, "src/main.rs");

        assert_eq!(state.tree_cursor, 1);
    }

    #[test]
    fn restore_cursor_fallback_to_parent() {
        let mut state = make_state();
        state.flat_nodes = vec![
            make_flat_node("README.md"),
            make_flat_node("src"),
            make_flat_node("tests"),
        ];
        state.tree_cursor = 0;

        // "src/main.rs" は存在しないが、親 "src" にフォールバック
        restore_cursor_position(&mut state, "src/main.rs");

        assert_eq!(state.tree_cursor, 1);
    }

    #[test]
    fn restore_cursor_no_match_keeps_original() {
        let mut state = make_state();
        state.flat_nodes = vec![make_flat_node("README.md"), make_flat_node("Cargo.toml")];
        state.tree_cursor = 0;

        // "vendor/lib.rs" もその親 "vendor" も存在しない
        restore_cursor_position(&mut state, "vendor/lib.rs");

        // tree_cursor は変わらない
        assert_eq!(state.tree_cursor, 0);
    }

    /// runtime にダミーのバッジスキャンエントリを挿入するヘルパー
    fn insert_dummy_badge_scan(runtime: &mut TuiRuntime, dir: &str) {
        let (_tx, rx) = mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        runtime.badge_scans.insert(
            dir.to_string(),
            BadgeScanEntry {
                receiver: rx,
                cancel_flag: flag,
            },
        );
    }

    /// テスト用のツリーを作成するヘルパー（ルート直下にファイルあり）
    fn make_tree_with_root_file() -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_file("a.txt")],
        }
    }

    /// テスト用のツリーを作成するヘルパー（ルート直下 + 展開可能なディレクトリ）
    fn make_tree_with_dir() -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![
                FileNode::new_dir_with_children("src", vec![FileNode::new_file("main.rs")]),
                FileNode::new_file("a.txt"),
            ],
        }
    }

    #[test]
    fn restore_tree_state_starts_root_badge_scan() {
        let mut runtime = TuiRuntime::new_for_test();
        let left = make_tree_with_root_file();
        let right = make_tree_with_root_file();
        let mut state = AppState::new(
            left,
            right,
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        );
        state.rebuild_flat_nodes();

        let expanded = HashSet::new();
        let left_source = Side::Local;
        let right_source = Side::Remote("test".to_string());

        restore_tree_state(
            &mut state,
            &mut runtime,
            expanded,
            None,
            0,
            &left_source,
            &right_source,
        );

        // ルート直下のバッジスキャンが起動されている
        assert!(
            runtime.badge_scans.contains_key(""),
            "root badge scan should be started"
        );
    }

    #[test]
    fn restore_tree_state_starts_expanded_dir_badge_scan() {
        let mut runtime = TuiRuntime::new_for_test();
        let left = make_tree_with_dir();
        let right = make_tree_with_dir();
        let mut state = AppState::new(
            left,
            right,
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        );
        state.expanded_dirs.insert("src".to_string());
        state.rebuild_flat_nodes();

        let mut expanded = HashSet::new();
        expanded.insert("src".to_string());
        let left_source = Side::Local;
        let right_source = Side::Remote("test".to_string());

        restore_tree_state(
            &mut state,
            &mut runtime,
            expanded,
            None,
            0,
            &left_source,
            &right_source,
        );

        // 展開済みディレクトリのバッジスキャンが起動されている
        assert!(
            runtime.badge_scans.contains_key("src"),
            "expanded dir badge scan should be started"
        );
        // ルート直下も起動されている
        assert!(
            runtime.badge_scans.contains_key(""),
            "root badge scan should also be started"
        );
    }

    #[test]
    fn restore_tree_state_no_expanded_dirs_still_scans_root() {
        let mut runtime = TuiRuntime::new_for_test();
        let left = make_tree_with_root_file();
        let right = make_tree_with_root_file();
        let mut state = AppState::new(
            left,
            right,
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        );
        state.rebuild_flat_nodes();

        let expanded = HashSet::new();
        let left_source = Side::Local;
        let right_source = Side::Remote("test".to_string());

        restore_tree_state(
            &mut state,
            &mut runtime,
            expanded,
            None,
            0,
            &left_source,
            &right_source,
        );

        // 展開ディレクトリが0でもルート直下のスキャンは起動される
        assert!(
            runtime.badge_scans.contains_key(""),
            "root badge scan should be started even with no expanded dirs"
        );
    }

    #[test]
    fn server_switch_cancels_existing_badge_scans() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = make_state();

        // ダミーのバッジスキャンエントリを挿入
        insert_dummy_badge_scan(&mut runtime, "src");
        insert_dummy_badge_scan(&mut runtime, "lib");
        assert_eq!(runtime.badge_scans.len(), 2);

        // execute_server_switch は Remote 接続に失敗して早期リターンするが、
        // cancel_all_badge_scans は接続前に呼ばれるのでクリアされる
        execute_server_switch(&mut state, &mut runtime, "staging");
        assert!(
            runtime.badge_scans.is_empty(),
            "badge scans should be cancelled before server switch"
        );
    }

    #[test]
    fn ref_swap_cancels_existing_badge_scans() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = make_state();
        // ref_source を設定（swap_right_ref が動作するために必要）
        state.ref_source = Some(Side::Remote("ref-server".to_string()));
        state.ref_tree = Some(make_tree_with_root_file());

        // ダミーのバッジスキャンエントリを挿入
        insert_dummy_badge_scan(&mut runtime, "src");
        insert_dummy_badge_scan(&mut runtime, "lib");
        assert_eq!(runtime.badge_scans.len(), 2);

        // execute_ref_swap はスワップ前にバッジスキャンをキャンセルする
        execute_ref_swap(&mut state, &mut runtime);

        // swap 後に新しいスキャンが起動されるため badge_scans は空ではないが、
        // 旧エントリ（src, lib）はキャンセルされている。
        // swap 後にルート直下のスキャンが起動されるため、"" キーは存在する。
        // 旧エントリがキャンセルされたことは、cancel_all_badge_scans が
        // swap_right_ref の前に呼ばれることで保証される。
        // ここでは cancel 後に新しいスキャンだけが残ることを確認する。
        assert!(
            !runtime.badge_scans.contains_key("src"),
            "old badge scan 'src' should be cancelled"
        );
        assert!(
            !runtime.badge_scans.contains_key("lib"),
            "old badge scan 'lib' should be cancelled"
        );
    }

    #[test]
    fn ref_swap_starts_badge_scans_for_expanded_dirs() {
        let mut runtime = TuiRuntime::new_for_test();
        let left = make_tree_with_dir();
        let right = make_tree_with_dir();
        let mut state = AppState::new(
            left,
            right,
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        );
        // ref_source を設定（swap_right_ref が動作するために必要）
        state.ref_source = Some(Side::Remote("ref-server".to_string()));
        state.ref_tree = Some(make_tree_with_dir());

        // 展開済みディレクトリを設定
        state.expanded_dirs.insert("src".to_string());
        state.rebuild_flat_nodes();

        execute_ref_swap(&mut state, &mut runtime);

        // 展開済みディレクトリのバッジスキャンが起動されている
        assert!(
            runtime.badge_scans.contains_key("src"),
            "expanded dir badge scan should be started after ref swap"
        );
    }

    #[test]
    fn ref_swap_starts_root_badge_scan() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = make_state();
        // ref_source を設定（swap_right_ref が動作するために必要）
        state.ref_source = Some(Side::Remote("ref-server".to_string()));
        state.ref_tree = Some(make_tree_with_root_file());
        state.rebuild_flat_nodes();

        execute_ref_swap(&mut state, &mut runtime);

        // ルート直下のバッジスキャンが起動されている
        assert!(
            runtime.badge_scans.contains_key(""),
            "root badge scan should be started after ref swap"
        );
    }

    #[test]
    fn pair_switch_cancels_existing_badge_scans() {
        let mut runtime = TuiRuntime::new_for_test();
        let mut state = make_state();

        // ダミーのバッジスキャンエントリを挿入
        insert_dummy_badge_scan(&mut runtime, "src");
        insert_dummy_badge_scan(&mut runtime, "tests");
        assert_eq!(runtime.badge_scans.len(), 2);

        // execute_pair_switch は接続に失敗して早期リターンするが、
        // cancel_all_badge_scans は接続前に呼ばれるのでクリアされる
        execute_pair_switch(&mut state, &mut runtime, "staging", "release");
        assert!(
            runtime.badge_scans.is_empty(),
            "badge scans should be cancelled before pair switch"
        );
    }

    #[test]
    fn reconnect_side_local_applies_include_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::create_dir(tmp.path().join("docs")).unwrap();
        std::fs::create_dir(tmp.path().join("vendor")).unwrap();

        let mut runtime = TuiRuntime::new_for_test();
        runtime.core.config.local.root_dir = tmp.path().to_path_buf();
        runtime.core.config.filter.include = vec!["src".to_string()];

        let left = FileTree {
            root: tmp.path().to_path_buf(),
            nodes: vec![],
        };
        let right = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_file("a.txt")],
        };
        let mut state = AppState::new(
            left,
            right,
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        );

        let ok = reconnect_side(&mut state, &mut runtime, &Side::Local, true);
        assert!(ok, "reconnect should succeed");

        // include フィルタが適用されて src のみ残る
        let names: Vec<&str> = state
            .left_tree
            .nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert!(names.contains(&"src"), "src should be kept");
        assert!(!names.contains(&"docs"), "docs should be filtered out");
        assert!(!names.contains(&"vendor"), "vendor should be filtered out");
    }
}
