//! 接続管理（再接続・サーバ切替）。

use crate::app::AppState;
use crate::runtime::TuiRuntime;

/// SSH 再接続を実行する（c キー）
pub fn execute_reconnect(state: &mut AppState, runtime: &mut TuiRuntime) {
    let server_name = state.server_name.clone();
    state.status_message = format!("Reconnecting to {}...", server_name);

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
                state.error_paths.clear();
                state.current_diff = None;
                state.selected_path = None;
                state.diff_scroll = 0;
                state.hunk_cursor = 0;
                state.pending_hunk_merge = None;
                state.undo_stack.clear();
                state.is_connected = true;
                state.clear_scan_cache();
                state.rebuild_flat_nodes();
                state.status_message = format!(
                    "Reconnected: {} | unsaved changes have been reset",
                    server_name
                );
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
