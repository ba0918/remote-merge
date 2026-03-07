//! マージ用ツリー遅延ロード。
//!
//! リモートディレクトリの遅延読み込みと、
//! マージ前のサブツリー再帰展開を担当する。

use crate::app::AppState;
use crate::runtime::TuiRuntime;

/// リモートディレクトリの遅延読み込み（ツリー側指定版）
///
/// `is_left` が true なら left_tree に、false なら right_tree にロードする。
pub fn load_remote_children_to(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    rel_path: &str,
    server_name: &str,
    is_left: bool,
) {
    let server_config = match runtime.get_server_config(server_name) {
        Ok(c) => c,
        Err(_) => return,
    };
    let remote_root = server_config.root_dir.to_string_lossy().to_string();
    let full_path = format!("{}/{}", remote_root.trim_end_matches('/'), rel_path);
    let exclude = state.active_exclude_patterns();

    let client = match runtime.core.ssh_clients.get_mut(server_name) {
        Some(c) => c,
        None => return,
    };

    let tree = if is_left {
        &mut state.left_tree
    } else {
        &mut state.right_tree
    };

    match runtime
        .core
        .rt
        .block_on(client.list_dir(&full_path, &exclude))
    {
        Ok(children) => {
            if let Some(node) = tree.find_node_mut(std::path::Path::new(rel_path)) {
                node.children = Some(children);
                node.sort_children();
            }
        }
        Err(e) => {
            tracing::debug!("Remote directory load skipped: {} - {}", rel_path, e);
            if crate::error::is_connection_error(&e) {
                state.is_connected = false;
                state.status_message = format!("Connection lost: {} | Press 'c' to reconnect", e);
            } else {
                state.status_message = format!("Remote dir load failed: {} - {}", rel_path, e);
            }
        }
    }
}

/// ディレクトリ配下の未ロードサブディレクトリを再帰的にロードする
///
/// マージ時に未展開ディレクトリの子もマージ対象にするため、
/// ツリー構造上の全サブディレクトリを遅延読み込みする。
pub fn expand_subtree_for_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    dir_path: &str,
) -> usize {
    let mut loaded = 0usize;
    let mut dirs_to_load: Vec<String> = vec![dir_path.to_string()];

    let left_server = state.left_source.server_name().map(|s| s.to_string());
    let right_server = state.right_source.server_name().map(|s| s.to_string());

    while let Some(path) = dirs_to_load.pop() {
        // 左側の未ロード子を読み込み
        let left_needs_load = state
            .left_tree
            .find_node(std::path::Path::new(&path))
            .is_some_and(|n| n.is_dir() && !n.is_loaded());
        if left_needs_load {
            if state.left_source.is_local() {
                state.load_local_children(&path);
            } else if let Some(ref name) = left_server {
                load_remote_children_to(state, runtime, &path, name, true);
            }
            loaded += 1;
        }

        // 右側の未ロード子を読み込み
        if state.is_connected {
            let right_needs_load = state
                .right_tree
                .find_node(std::path::Path::new(&path))
                .is_some_and(|n| n.is_dir() && !n.is_loaded());
            if right_needs_load {
                if let Some(ref name) = right_server {
                    load_remote_children_to(state, runtime, &path, name, false);
                }
                loaded += 1;
            }
        }

        // NOTE: expanded_dirs には追加しない（ツリー表示の展開状態を変えない）
        // ファイル収集は collect_merge_files() がツリーから直接行う

        // 左右ツリーのサブディレクトリを収集（重複排除に HashSet 使用）
        let mut sub_dirs = std::collections::HashSet::new();
        for tree in [&state.left_tree, &state.right_tree] {
            if let Some(node) = tree.find_node(std::path::Path::new(&path)) {
                if let Some(children) = &node.children {
                    for child in children {
                        if child.is_dir() {
                            sub_dirs.insert(format!("{}/{}", path, child.name));
                        }
                    }
                }
            }
        }

        dirs_to_load.extend(sub_dirs);
    }

    state.rebuild_flat_nodes();
    loaded
}
