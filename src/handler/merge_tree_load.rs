//! マージ用ツリー遅延ロード。
//!
//! リモートディレクトリの遅延読み込みと、
//! マージ前のサブツリー再帰展開を担当する。
//! ref_tree の同期ロード（3way マージ整合性維持）も含む。

use crate::app::AppState;
use crate::runtime::TuiRuntime;

/// Side ベースのディレクトリ遅延読み込み（統一 API 経由）
///
/// `is_left` が true なら left_tree に、false なら right_tree にロードする。
pub fn load_children_to(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    rel_path: &str,
    side: &crate::app::Side,
    is_left: bool,
) {
    let tree = if is_left {
        &mut state.left_tree
    } else {
        &mut state.right_tree
    };

    match runtime.fetch_children(side, rel_path) {
        Ok(children) => {
            if let Some(node) = tree.find_node_mut(std::path::Path::new(rel_path)) {
                node.children = Some(children);
                node.sort_children();
            }
        }
        Err(e) => {
            tracing::debug!("Directory load skipped: {} - {}", rel_path, e);
            if crate::error::is_connection_error(&e) {
                state.is_connected = false;
                runtime.disconnect_if_remote(side);
                state.status_message = format!("Connection lost: {} | Press 'c' to reconnect", e);
            } else {
                state.status_message = format!("Dir load failed: {} - {}", rel_path, e);
            }
        }
    }
}

/// ref_tree のディレクトリ children を遅延ロードする。
///
/// left/right のディレクトリ展開時に ref_tree も同期的にロードすることで、
/// `merge_node_lists_3way` でのツリー深度不整合を防ぐ。
/// ref_source が未設定、または対象ディレクトリが ref_tree に存在しない場合は何もしない。
pub fn load_ref_children(state: &mut AppState, runtime: &mut TuiRuntime, rel_path: &str) {
    let ref_source = match &state.ref_source {
        Some(source) => source.clone(),
        None => return,
    };

    let needs_load = state
        .ref_tree
        .as_ref()
        .and_then(|t| t.find_node(std::path::Path::new(rel_path)))
        .is_some_and(|n| n.is_dir() && !n.is_loaded());

    if !needs_load {
        return;
    }

    match runtime.fetch_children(&ref_source, rel_path) {
        Ok(children) => {
            if let Some(ref mut tree) = state.ref_tree {
                if let Some(node) = tree.find_node_mut(std::path::Path::new(rel_path)) {
                    node.children = Some(children);
                    node.sort_children();
                }
            }
        }
        Err(e) => {
            if crate::error::is_connection_error(&e) {
                tracing::warn!(
                    "Ref directory load failed (connection): {} - {}",
                    rel_path,
                    e
                );
            } else {
                tracing::debug!("Ref directory load skipped: {} - {}", rel_path, e);
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

    let left_source = state.left_source.clone();
    let right_source = state.right_source.clone();

    while let Some(path) = dirs_to_load.pop() {
        // 左側: 中間ディレクトリがなければ作成してから未ロード子を読み込み
        state.left_tree.ensure_path(std::path::Path::new(&path));
        let left_needs_load = state
            .left_tree
            .find_node(std::path::Path::new(&path))
            .is_some_and(|n| n.is_dir() && !n.is_loaded());
        if left_needs_load {
            load_children_to(state, runtime, &path, &left_source, true);
            loaded += 1;
        }

        // 右側: 中間ディレクトリがなければ作成してから未ロード子を読み込み
        if runtime.is_side_available(&right_source) {
            state.right_tree.ensure_path(std::path::Path::new(&path));
            let right_needs_load = state
                .right_tree
                .find_node(std::path::Path::new(&path))
                .is_some_and(|n| n.is_dir() && !n.is_loaded());
            if right_needs_load {
                load_children_to(state, runtime, &path, &right_source, false);
                loaded += 1;
            }
        }

        // ref_tree も同期ロード（3way マージ整合性維持）
        load_ref_children(state, runtime, &path);

        // NOTE: expanded_dirs には追加しない（ツリー表示の展開状態を変えない）
        // ファイル収集は collect_merge_files() がツリーから直接行う

        // 左右+ref ツリーのサブディレクトリを収集（重複排除に HashSet 使用）
        let mut sub_dirs = std::collections::HashSet::new();
        for tree in std::iter::once(&state.left_tree)
            .chain(std::iter::once(&state.right_tree))
            .chain(state.ref_tree.as_ref())
        {
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
