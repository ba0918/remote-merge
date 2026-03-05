//! マージ実行・ファイルコンテンツ読み込み。

use crate::app::AppState;
use crate::diff::engine::HunkDirection;
use crate::merge::executor::{self, MergeDirection};
use crate::runtime::TuiRuntime;
use crate::ui::dialog::{BatchConfirmDialog, ConfirmDialog};

/// マージを実行する
pub fn execute_merge(state: &mut AppState, runtime: &mut TuiRuntime, confirm: &ConfirmDialog) {
    let path = &confirm.file_path;
    let direction = confirm.direction;

    match direction {
        MergeDirection::LocalToRemote => {
            let content = match state.local_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    state.status_message = format!("{}: local content not loaded", path);
                    return;
                }
            };

            if !state.is_connected {
                state.status_message = "SSH not connected: cannot merge".to_string();
                return;
            }

            match runtime.write_remote_file(&state.server_name, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    state.status_message =
                        format!("{}: local -> {} merged", path, state.server_name);
                }
                Err(e) => {
                    state.status_message = format!("Merge failed: {}", e);
                }
            }
        }
        MergeDirection::RemoteToLocal => {
            let content = match state.remote_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    state.status_message = format!("{}: remote content not loaded", path);
                    return;
                }
            };

            let local_root = state.local_tree.root.clone();
            match executor::write_local_file(&local_root, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    state.status_message =
                        format!("{}: {} -> local merged", path, state.server_name);
                }
                Err(e) => {
                    state.status_message = format!("Merge failed: {}", e);
                }
            }
        }
    }
}

/// バッチマージを実行する（ディレクトリ選択時）
pub fn execute_batch_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    batch: &BatchConfirmDialog,
) {
    let direction = batch.direction;
    let file_count = batch.files.len();
    let mut success_count = 0usize;
    let mut fail_count = 0usize;

    for (i, (path, _badge)) in batch.files.iter().enumerate() {
        state.status_message = format!("Merging... {}/{}", i + 1, file_count);

        match direction {
            MergeDirection::LocalToRemote => {
                let content = match state.local_cache.get(path) {
                    Some(c) => c.clone(),
                    None => {
                        let local_root = &state.local_tree.root;
                        match executor::read_local_file(local_root, path) {
                            Ok(c) => {
                                state.local_cache.insert(path.clone(), c.clone());
                                c
                            }
                            Err(_) => {
                                fail_count += 1;
                                continue;
                            }
                        }
                    }
                };

                if !state.is_connected {
                    state.status_message = format!(
                        "SSH disconnected: results so far: {} succeeded/{} failed",
                        success_count, fail_count
                    );
                    return;
                }

                match runtime.write_remote_file(&state.server_name, path, &content) {
                    Ok(()) => {
                        state.update_badge_after_merge(path, &content, direction);
                        success_count += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Batch merge failed: {} - {}", path, e);
                        fail_count += 1;
                    }
                }
            }
            MergeDirection::RemoteToLocal => {
                let content = match state.remote_cache.get(path) {
                    Some(c) => c.clone(),
                    None => {
                        if !state.is_connected {
                            state.status_message = format!(
                                "SSH disconnected: results so far: {} succeeded/{} failed",
                                success_count, fail_count
                            );
                            return;
                        }
                        match runtime.read_remote_file(&state.server_name, path) {
                            Ok(c) => {
                                state.remote_cache.insert(path.clone(), c.clone());
                                c
                            }
                            Err(_) => {
                                fail_count += 1;
                                continue;
                            }
                        }
                    }
                };

                let local_root = state.local_tree.root.clone();
                match executor::write_local_file(&local_root, path, &content) {
                    Ok(()) => {
                        state.update_badge_after_merge(path, &content, direction);
                        success_count += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Batch merge failed: {} - {}", path, e);
                        fail_count += 1;
                    }
                }
            }
        }
    }

    let dir_str = match direction {
        MergeDirection::LocalToRemote => format!("local -> {}", state.server_name),
        MergeDirection::RemoteToLocal => format!("{} -> local", state.server_name),
    };

    if fail_count == 0 {
        state.status_message = format!(
            "Batch merge complete: {} files merged ({})",
            success_count, dir_str
        );
    } else {
        state.status_message = format!(
            "Batch merge complete: {} succeeded/{} failed ({})",
            success_count, fail_count, dir_str
        );
    }
}

/// ハンクマージを実行する（2段階操作の確定時）
pub fn execute_hunk_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    direction: HunkDirection,
) {
    if let Some(path) = state.apply_hunk_merge(direction) {
        match direction {
            HunkDirection::RightToLeft => {
                let content = state.local_cache.get(&path).cloned().unwrap_or_default();
                let local_root = state.local_tree.root.clone();
                match executor::write_local_file(&local_root, &path, &content) {
                    Ok(()) => {
                        state.status_message = format!(
                            "Hunk merged: remote -> local ({}) | {} hunks left",
                            path,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        state.status_message = format!("Local write failed: {}", e);
                    }
                }
            }
            HunkDirection::LeftToRight => {
                let content = state.remote_cache.get(&path).cloned().unwrap_or_default();
                match runtime.write_remote_file(&state.server_name, &path, &content) {
                    Ok(()) => {
                        state.status_message = format!(
                            "Hunk merged: local -> remote ({}) | {} hunks left",
                            path,
                            state.hunk_count(),
                        );
                    }
                    Err(e) => {
                        state.status_message = format!("Remote write failed: {}", e);
                    }
                }
            }
        }
    }
}

/// 変更をファイルに書き込む（w キー確定後）
pub fn execute_write_changes(state: &mut AppState, runtime: &mut TuiRuntime) {
    if let Some(path) = state.selected_path.clone() {
        let changes = state.undo_stack.len();

        if let Some(local_content) = state.local_cache.get(&path) {
            let local_root = state.local_tree.root.clone();
            if let Err(e) = executor::write_local_file(&local_root, &path, local_content) {
                state.status_message = format!("Local write failed: {}", e);
                return;
            }
        }

        if state.is_connected {
            if let Some(remote_content) = state.remote_cache.get(&path).cloned() {
                if let Err(e) =
                    runtime.write_remote_file(&state.server_name, &path, &remote_content)
                {
                    state.status_message = format!("Remote write failed: {}", e);
                    return;
                }
            }
        }

        state.undo_stack.clear();
        state.status_message = format!(
            "{}: {} changes written | {} hunks remaining",
            path,
            changes,
            state.hunk_count()
        );
    }
}

/// リモートディレクトリの遅延読み込み
pub fn load_remote_children(state: &mut AppState, runtime: &mut TuiRuntime, rel_path: &str) {
    let server_name = state.server_name.clone();
    let server_config = match runtime.get_server_config(&server_name) {
        Ok(c) => c,
        Err(_) => return,
    };
    let remote_root = server_config.root_dir.to_string_lossy().to_string();
    let full_path = format!("{}/{}", remote_root.trim_end_matches('/'), rel_path);
    let exclude = state.active_exclude_patterns();

    let client = match runtime.ssh_client.as_mut() {
        Some(c) => c,
        None => return,
    };

    match runtime.rt.block_on(client.list_dir(&full_path, &exclude)) {
        Ok(children) => {
            if let Some(node) = state
                .remote_tree
                .find_node_mut(std::path::Path::new(rel_path))
            {
                node.children = Some(children);
                node.sort_children();
            }
        }
        Err(e) => {
            tracing::debug!("Remote directory load skipped: {} - {}", rel_path, e);
            state.is_connected = false;
            state.status_message = format!("Connection lost: {} | Press 'c' to reconnect", e);
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

    while let Some(path) = dirs_to_load.pop() {
        // ローカルの未ロード子を読み込み
        let local_needs_load = state
            .local_tree
            .find_node(std::path::Path::new(&path))
            .is_some_and(|n| n.is_dir() && !n.is_loaded());
        if local_needs_load {
            state.load_local_children(&path);
            loaded += 1;
        }

        // リモートの未ロード子を読み込み
        if state.is_connected {
            let remote_needs_load = state
                .remote_tree
                .find_node(std::path::Path::new(&path))
                .is_some_and(|n| n.is_dir() && !n.is_loaded());
            if remote_needs_load {
                load_remote_children(state, runtime, &path);
                loaded += 1;
            }
        }

        // 展開状態に追加（表示のため）
        state.expanded_dirs.insert(path.clone());

        // ローカルツリーのサブディレクトリを収集
        let mut sub_dirs = Vec::new();
        if let Some(node) = state.local_tree.find_node(std::path::Path::new(&path)) {
            if let Some(children) = &node.children {
                for child in children {
                    if child.is_dir() {
                        sub_dirs.push(format!("{}/{}", path, child.name));
                    }
                }
            }
        }

        // リモートツリーのサブディレクトリも収集（ローカルにないものも含む）
        if let Some(node) = state.remote_tree.find_node(std::path::Path::new(&path)) {
            if let Some(children) = &node.children {
                for child in children {
                    if child.is_dir() {
                        let child_path = format!("{}/{}", path, child.name);
                        if !sub_dirs.contains(&child_path) {
                            sub_dirs.push(child_path);
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

/// ディレクトリ配下の全ファイルのコンテンツをロードする（マージ準備用）
///
/// flat_nodes から指定ディレクトリ配下のファイルを収集し、
/// ローカル・リモート両方のコンテンツをキャッシュに読み込む。
pub fn load_subtree_contents(state: &mut AppState, runtime: &mut TuiRuntime, dir_path: &str) {
    let prefix = format!("{}/", dir_path);
    let file_paths: Vec<String> = state
        .flat_nodes
        .iter()
        .filter(|n| !n.is_dir && n.path.starts_with(&prefix))
        .map(|n| n.path.clone())
        .collect();

    let total = file_paths.len();
    for (i, path) in file_paths.iter().enumerate() {
        if i % 10 == 0 {
            state.status_message = format!("Loading files... {}/{}", i, total);
        }

        // ローカルコンテンツ
        let local_loaded = state.local_cache.contains_key(path);
        if !local_loaded {
            let local_root = &state.local_tree.root;
            match executor::read_local_file(local_root, path) {
                Ok(content) => {
                    state.local_cache.insert(path.clone(), content);
                    state.error_paths.remove(path);
                }
                Err(e) => {
                    tracing::debug!("Local file read skipped: {} - {}", path, e);
                }
            }
        }

        // リモートコンテンツ
        let remote_loaded = state.remote_cache.contains_key(path);
        if !remote_loaded && state.is_connected {
            match runtime.read_remote_file(&state.server_name, path) {
                Ok(content) => {
                    state.remote_cache.insert(path.clone(), content);
                    state.error_paths.remove(path);
                }
                Err(e) => {
                    tracing::debug!("Remote file read skipped: {} - {}", path, e);
                    state.is_connected = false;
                    state.status_message =
                        format!("Connection lost: {} | Press 'c' to reconnect", e);
                    return;
                }
            }
        }

        // 両方とも読み込めなかった場合のみエラー扱い
        let has_local = state.local_cache.contains_key(path);
        let has_remote = state.remote_cache.contains_key(path);
        if !has_local && !has_remote {
            state.error_paths.insert(path.clone());
        }
    }

    state.rebuild_flat_nodes();
}

/// ファイル選択時にコンテンツをロードする
pub fn load_file_content(state: &mut AppState, runtime: &mut TuiRuntime) {
    let node = match state.flat_nodes.get(state.tree_cursor) {
        Some(n) if !n.is_dir => n.clone(),
        _ => return,
    };

    let path = &node.path;

    // ローカルキャッシュ
    if !state.local_cache.contains_key(path) {
        let local_root = &state.local_tree.root;
        match executor::read_local_file(local_root, path) {
            Ok(content) => {
                state.local_cache.insert(path.clone(), content);
                state.error_paths.remove(path);
            }
            Err(e) => {
                tracing::debug!("Local file read skipped: {} - {}", path, e);
                state.status_message = format!("Local read failed: {} - {}", path, e);
            }
        }
    }

    // リモートキャッシュ
    if !state.remote_cache.contains_key(path) && state.is_connected {
        match runtime.read_remote_file(&state.server_name, path) {
            Ok(content) => {
                state.remote_cache.insert(path.clone(), content);
                state.error_paths.remove(path);
            }
            Err(e) => {
                tracing::debug!("Remote file read skipped: {} - {}", path, e);
                state.is_connected = false;
                state.status_message = format!("Connection lost: {} | Press 'c' to reconnect", e);
            }
        }
    }

    // 両方とも読み込めなかった場合のみエラー扱い
    if !state.local_cache.contains_key(path) && !state.remote_cache.contains_key(path) {
        state.error_paths.insert(path.clone());
    }

    state.rebuild_flat_nodes();
}
