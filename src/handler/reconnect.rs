//! 接続管理（再接続・サーバ切替）。

use crate::app::AppState;
use crate::runtime::TuiRuntime;

/// SSH 再接続を実行する（c キー）
pub fn execute_reconnect(state: &mut AppState, runtime: &mut TuiRuntime) {
    let server_name = state.server_name.clone();
    state.status_message = format!("Reconnecting to {}...", server_name);

    // 展開状態とカーソル位置を保存
    let expanded_backup = state.expanded_dirs.clone();
    let cursor_path = state.current_path();

    runtime.disconnect();

    let local_tree_result = crate::local::scan_local_tree(
        &runtime.config.local.root_dir,
        &runtime.config.filter.exclude,
    );

    match runtime.connect(&server_name) {
        Ok(()) => match runtime.fetch_remote_tree(&server_name) {
            Ok(tree) => {
                if let Ok(local_tree) = local_tree_result {
                    state.local_tree = local_tree;
                }
                state.remote_tree = tree;
                state.local_cache.clear();
                state.remote_cache.clear();
                state.local_binary_cache.clear();
                state.remote_binary_cache.clear();
                state.error_paths.clear();
                state.current_diff = None;
                state.selected_path = None;
                state.diff_scroll = 0;
                state.hunk_cursor = 0;
                state.pending_hunk_merge = None;
                state.undo_stack.clear();
                state.is_connected = true;
                state.clear_scan_cache();

                // 展開状態を復元
                state.expanded_dirs = expanded_backup;

                // 展開済みディレクトリの子ノードを再取得
                let dirs: Vec<String> = state.expanded_dirs.iter().cloned().collect();
                for dir in &dirs {
                    state.load_local_children(dir);
                    super::merge_exec::load_remote_children(state, runtime, dir);
                }

                state.rebuild_flat_nodes();

                // カーソル位置を復元
                if let Some(path) = cursor_path {
                    restore_cursor_position(state, &path);
                }

                state.status_message =
                    format!("Reconnected: {} | tree state restored", server_name);
            }
            Err(e) => {
                state.is_connected = false;
                state.status_message = format!("{} tree fetch failed: {}", server_name, e);
            }
        },
        Err(e) => {
            state.is_connected = false;
            state.status_message = format!("{} reconnection failed: {} | c: retry", server_name, e);
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

/// サーバ切替を実行する
pub fn execute_server_switch(state: &mut AppState, runtime: &mut TuiRuntime, server_name: &str) {
    state.status_message = format!("Connecting to {}...", server_name);

    runtime.disconnect();

    match runtime.connect(server_name) {
        Ok(()) => match runtime.fetch_remote_tree(server_name) {
            Ok(tree) => {
                state.switch_server(server_name.to_string(), tree);
                state.status_message = format!(
                    "local <-> {} | Tab: switch focus | s: server | q: quit",
                    server_name
                );
            }
            Err(e) => {
                state.status_message = format!("{} tree fetch failed: {}", server_name, e);
            }
        },
        Err(e) => {
            state.status_message = format!("{} connection failed: {}", server_name, e);
        }
    }
}
