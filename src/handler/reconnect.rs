//! 接続管理（再接続・サーバ切替）。

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
    state.showing_ref_diff = false;
    state.clear_scan_cache();

    // 展開状態を復元
    state.expanded_dirs = expanded_backup;

    // 展開済みディレクトリの子ノードを再取得
    let dirs: Vec<String> = state.expanded_dirs.iter().cloned().collect();
    for dir in &dirs {
        // 左側: ローカルならファイルシステムから、リモートならSSHで取得
        match &left_source {
            Side::Local => {
                state.load_local_children(dir);
            }
            Side::Remote(name) => {
                super::merge_exec::load_remote_children_to(state, runtime, dir, name, true);
            }
        }
        // 右側: 常にリモートから取得
        if let Side::Remote(name) = &right_source {
            super::merge_exec::load_remote_children_to(state, runtime, dir, name, false);
        }
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

    // Side を構築
    let new_left = if left_name == "local" {
        Side::Local
    } else {
        Side::Remote(left_name.to_string())
    };
    let new_right = if right_name == "local" {
        Side::Local
    } else {
        Side::Remote(right_name.to_string())
    };

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

    // reference サーバが自動選択されたら接続 + ツリー取得
    execute_ref_connect(state, runtime);

    let label = comparison_label(&state.left_source, &state.right_source);
    state.status_message = format!("{} | Tab: switch focus | s: server | q: quit", label);
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

    let tree_acquired = match &ref_source {
        Side::Local => {
            // reference は再帰的に全走査する（浅いスキャンだと find_node が失敗する）
            match crate::local::scan_local_tree_recursive(
                &runtime.core.config.local.root_dir,
                &runtime.core.config.filter.exclude,
                10_000,
            ) {
                Ok((nodes, _truncated)) => {
                    let mut tree = crate::tree::FileTree::new(&runtime.core.config.local.root_dir);
                    tree.nodes = nodes;
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
            // reference ツリーは再帰取得（遅延読み込みだと find_node が失敗するため）
            match runtime.fetch_remote_tree_recursive(name, 10_000) {
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
