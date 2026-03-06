//! 接続管理（再接続・サーバ切替）。

use crate::app::side::comparison_label;
use crate::app::{AppState, Side};
use crate::runtime::TuiRuntime;

/// SSH 再接続を実行する（c キー）
///
/// left_source / right_source に応じて必要な接続を再確立する。
pub fn execute_reconnect(state: &mut AppState, runtime: &mut TuiRuntime) {
    let left_source = state.left_source.clone();
    let right_source = state.right_source.clone();
    let label = comparison_label(&left_source, &right_source);
    state.status_message = format!("Reconnecting ({})...", label);

    // 展開状態とカーソル位置を保存
    let expanded_backup = state.expanded_dirs.clone();
    let cursor_path = state.current_path();

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
    state.clear_scan_cache();

    // 展開状態を復元
    state.expanded_dirs = expanded_backup;

    // 展開済みディレクトリの子ノードを再取得
    let dirs: Vec<String> = state.expanded_dirs.iter().cloned().collect();
    for dir in &dirs {
        if left_source.is_local() {
            state.load_local_children(dir);
        }
        // リモート側の子ノードは right_tree に対して取得
        super::merge_exec::load_remote_children(state, runtime, dir);
    }

    state.rebuild_flat_nodes();

    // カーソル位置を復元
    if let Some(path) = cursor_path {
        restore_cursor_position(state, &path);
    }

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
                &runtime.config.local.root_dir,
                &runtime.config.filter.exclude,
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
                    state.status_message =
                        format!("{} reconnection failed: {} | c: retry", server_name, e);
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

/// サーバ切替を実行する（右側のサーバを切り替え）
pub fn execute_server_switch(state: &mut AppState, runtime: &mut TuiRuntime, server_name: &str) {
    state.status_message = format!("Connecting to {}...", server_name);

    // 古い右側接続を切断
    if let Side::Remote(old_name) = &state.right_source {
        runtime.disconnect(old_name);
    }

    match runtime.connect(server_name) {
        Ok(()) => match runtime.fetch_remote_tree(server_name) {
            Ok(tree) => {
                state.right_source = Side::Remote(server_name.to_string());
                state.switch_server(server_name.to_string(), tree);
                let label = comparison_label(&state.left_source, &state.right_source);
                state.status_message =
                    format!("{} | Tab: switch focus | s: server | q: quit", label);
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
