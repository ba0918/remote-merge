//! 接続管理（再接続・サーバ切替）。

use std::collections::HashSet;

use crate::app::side::comparison_label;
use crate::app::{AppState, Side};
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
                Ok(tree) => {
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
                    Ok(tree) => {
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

    // Side を構築
    let new_left = Side::new(left_name);
    let new_right = Side::new(right_name);

    // 左側ツリーを取得
    let left_tree = match &new_left {
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
    let right_tree = match &new_right {
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
                Ok(tree) => {
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
                Ok(tree) => {
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

/// サーバ切替を実行する（右側のサーバを切り替え）
pub fn execute_server_switch(state: &mut AppState, runtime: &mut TuiRuntime, server_name: &str) {
    let new_side = Side::new(server_name);

    state.status_message = format!("Switching to {}...", server_name);

    // 切替前にツリー状態をバックアップ
    let expanded_backup = state.expanded_dirs.clone();
    let cursor_path = state.current_path();
    let scroll_backup = state.tree_scroll;

    // 古い右側接続を切断
    if let Side::Remote(old_name) = &state.right_source {
        runtime.disconnect(old_name);
    }

    // Side に応じてツリーを取得
    let tree = match &new_side {
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
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

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
}
