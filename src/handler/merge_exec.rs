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
    let server_config = match runtime.config.servers.get(&server_name) {
        Some(c) => c,
        None => return,
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
            state.status_message = format!("Remote directory load failed: {} - {}", rel_path, e);
        }
    }
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
                state.error_paths.insert(path.clone());
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
                state.status_message = format!("Remote read failed: {} - {}", path, e);
                state.error_paths.insert(path.clone());
            }
        }
    }

    state.rebuild_flat_nodes();
}
