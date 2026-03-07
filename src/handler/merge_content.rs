//! マージ用コンテンツ読み込み。
//!
//! ファイルコンテンツのキャッシュロード（サブツリー一括・単一ファイル）を担当する。
//! ツリーの遅延ロード・展開は `merge_tree_load` に分離。

use crate::app::AppState;
use crate::merge::executor;
use crate::runtime::TuiRuntime;
use crate::ui::dialog::{DialogState, ProgressDialog, ProgressPhase};

use super::merge_file_io::{is_symlink_in_tree, read_left_file};

/// ディレクトリ配下の全ファイルのコンテンツをロードする（マージ準備用）
///
/// ツリーから直接ファイルパスを収集し（expanded_dirs に依存しない）、
/// ローカル・リモート両方のコンテンツをキャッシュに読み込む。
///
/// リモートファイルは **バッチ読み込み**（1つのSSHチャネルで全ファイル）を使い、
/// チャネル枯渇を防ぐ。
pub fn load_subtree_contents(state: &mut AppState, runtime: &mut TuiRuntime, dir_path: &str) {
    let file_paths: Vec<String> = crate::app::merge_collect::collect_merge_files(
        &state.left_tree,
        &state.right_tree,
        dir_path,
    )
    .into_iter()
    .filter(|p| !is_symlink_in_tree(state, p))
    .collect();

    let total = file_paths.len();
    let mut progress = ProgressDialog::new(ProgressPhase::LoadingFiles, "", false);
    progress.total = Some(total);
    state.dialog = DialogState::Progress(progress);

    // ── キャッシュをクリアして最新の内容を取得する ──
    state.invalidate_cache_for_paths(&file_paths);

    // ── 左側ファイルの読み込み ──
    load_left_files(state, runtime, &file_paths);

    // ── 右側ファイルをバッチ読み込み ──
    load_right_files(state, runtime, &file_paths);

    // ── エラーパス判定 ──
    for path in &file_paths {
        let has_local = state.left_cache.contains_key(path);
        let has_remote = state.right_cache.contains_key(path);
        if !has_local && !has_remote {
            state.error_paths.insert(path.clone());
        }
    }

    state.dialog = DialogState::None;
    state.rebuild_flat_nodes();
}

/// 左側ファイルの読み込み（ローカル個別 or リモートバッチ）
fn load_left_files(state: &mut AppState, runtime: &mut TuiRuntime, file_paths: &[String]) {
    if state.left_source.is_local() {
        for (i, path) in file_paths.iter().enumerate() {
            if i % 10 == 0 {
                if let DialogState::Progress(ref mut progress) = state.dialog {
                    progress.current = i;
                    progress.current_path = Some(path.clone());
                }
            }
            if !state.left_cache.contains_key(path) {
                let local_root = &state.left_tree.root;
                match executor::read_local_file(local_root, path) {
                    Ok(content) => {
                        state.left_cache.insert(path.clone(), content);
                        state.error_paths.remove(path);
                    }
                    Err(e) => {
                        tracing::debug!("Local file read skipped: {} - {}", path, e);
                    }
                }
            }
        }
    } else if let Some(left_server) = state.left_source.server_name().map(|s| s.to_string()) {
        let left_paths: Vec<String> = file_paths
            .iter()
            .filter(|p| !state.left_cache.contains_key(p))
            .cloned()
            .collect();

        if !left_paths.is_empty() {
            if let DialogState::Progress(ref mut progress) = state.dialog {
                progress.phase = ProgressPhase::LoadingRemote;
                progress.current = 0;
                progress.total = Some(left_paths.len());
            }

            match runtime.read_remote_files_batch(&left_server, &left_paths) {
                Ok(batch_result) => {
                    for (path, content) in batch_result {
                        state.left_cache.insert(path.clone(), content);
                        state.error_paths.remove(&path);
                    }
                }
                Err(e) => {
                    tracing::warn!("Left batch remote read failed: {}", e);
                    if crate::error::is_connection_error(&e) {
                        state.status_message =
                            format!("Left connection lost: {} | Press 'c' to reconnect", e);
                        state.dialog = DialogState::None;
                    }
                }
            }
        }
    }
}

/// 右側ファイルのバッチ読み込み
fn load_right_files(state: &mut AppState, runtime: &mut TuiRuntime, file_paths: &[String]) {
    let remote_paths: Vec<String> = file_paths
        .iter()
        .filter(|p| !state.right_cache.contains_key(p))
        .cloned()
        .collect();

    if remote_paths.is_empty() {
        return;
    }

    if !state.is_connected && runtime.check_connection(&state.server_name) {
        tracing::info!("SSH connection recovered during subtree load");
        state.is_connected = true;
    }

    if !state.is_connected {
        return;
    }

    if let DialogState::Progress(ref mut progress) = state.dialog {
        progress.phase = ProgressPhase::LoadingRemote;
        progress.current = 0;
        progress.total = Some(remote_paths.len());
    }

    match runtime.read_remote_files_batch(&state.server_name, &remote_paths) {
        Ok(batch_result) => {
            for (path, content) in batch_result {
                state.right_cache.insert(path.clone(), content);
                state.error_paths.remove(&path);
            }
        }
        Err(e) => {
            tracing::warn!("Right batch remote read failed: {}", e);
            if crate::error::is_connection_error(&e) {
                state.is_connected = false;
                state.status_message = format!("Connection lost: {} | Press 'c' to reconnect", e);
                state.dialog = DialogState::None;
            }
        }
    }
}

/// ファイル選択時にコンテンツをロードする。
///
/// 未保存の変更がない場合はキャッシュを無効化して毎回再取得する。
pub fn load_file_content(state: &mut AppState, runtime: &mut TuiRuntime) {
    let node = match state.flat_nodes.get(state.tree_cursor) {
        Some(n) if !n.is_dir => n.clone(),
        _ => return,
    };

    // シンボリックリンクはツリーノードから直接比較する
    if node.is_symlink {
        return;
    }

    let path = &node.path;
    let is_ref_only = node.ref_only;

    // 未保存変更がなければキャッシュを無効化して最新を取得
    if !state.has_unsaved_changes() {
        state.invalidate_cache_for_paths(std::slice::from_ref(path));
    }

    // ref-only ファイルは left/right に存在しないため読み込みをスキップ
    if !is_ref_only {
        // 左側キャッシュ（ローカル or リモート）
        if !state.left_cache.contains_key(path) {
            match read_left_file(state, runtime, path) {
                Ok(content) => {
                    state.left_cache.insert(path.clone(), content);
                    state.error_paths.remove(path);
                }
                Err(e) => {
                    tracing::debug!("Left file read skipped: {} - {}", path, e);
                    if state.left_source.is_remote() && crate::error::is_connection_error(&e) {
                        state.status_message =
                            format!("Left connection lost: {} | Press 'c' to reconnect", e);
                    } else {
                        state.status_message = format!("Left read failed: {} - {}", path, e);
                    }
                }
            }
        }

        // 右側キャッシュ（リモート）
        load_right_file_content(state, runtime, path);

        // 両方とも読み込めなかった場合のみエラー扱い
        if !state.left_cache.contains_key(path) && !state.right_cache.contains_key(path) {
            state.error_paths.insert(path.clone());
        }
    }

    // reference サーバのコンテンツを遅延取得（3way バッジ用 & ref-only 表示用）
    load_ref_file_content(state, runtime, path);

    state.rebuild_flat_nodes();
}

/// reference サーバの単一ファイルコンテンツをロードする（3way バッジ用）
///
/// reference サーバが未設定の場合は何もしない。
/// 取得失敗時はバッジ非表示（エラーにしない、graceful degradation）。
fn load_ref_file_content(state: &mut AppState, runtime: &mut TuiRuntime, path: &str) {
    if !state.has_reference() || state.ref_cache.contains_key(path) {
        return;
    }

    let ref_source = match &state.ref_source {
        Some(source) => source.clone(),
        None => return,
    };

    match &ref_source {
        crate::app::Side::Local => match read_local_file_for_ref(runtime, path) {
            Ok(content) => {
                state.ref_cache.insert(path.to_string(), content);
            }
            Err(e) => {
                tracing::debug!("Ref file read skipped: {} - {}", path, e);
            }
        },
        crate::app::Side::Remote(name) => match runtime.read_remote_file(name, path) {
            Ok(content) => {
                state.ref_cache.insert(path.to_string(), content);
            }
            Err(e) => {
                tracing::debug!("Ref remote file read skipped: {} - {}", path, e);
            }
        },
    }
}

/// reference がローカルの場合のファイル読み込み
fn read_local_file_for_ref(runtime: &TuiRuntime, path: &str) -> anyhow::Result<String> {
    let root_dir = &runtime.core.config.local.root_dir;
    executor::read_local_file(root_dir, path).map_err(|e| anyhow::anyhow!("{}", e))
}

/// 右側の単一ファイルコンテンツをロードする
fn load_right_file_content(state: &mut AppState, runtime: &mut TuiRuntime, path: &str) {
    if state.right_cache.contains_key(path) {
        return;
    }

    if !state.is_connected && runtime.check_connection(&state.server_name) {
        tracing::warn!("SSH connection recovered, resuming remote operations");
        state.is_connected = true;
    }

    if state.is_connected {
        match runtime.read_remote_file(&state.server_name, path) {
            Ok(content) => {
                state.right_cache.insert(path.to_string(), content);
                state.error_paths.remove(path);
            }
            Err(e) => {
                tracing::warn!("Right file read failed: {} - {}", path, e);
                if crate::error::is_connection_error(&e) {
                    state.is_connected = false;
                    state.status_message =
                        format!("Connection lost: {} | Press 'c' to reconnect", e);
                } else {
                    state.status_message = format!("Right read failed: {} - {}", path, e);
                }
            }
        }
    } else {
        tracing::warn!(
            path = %path,
            has_client = runtime.has_client(&state.server_name),
            "Right read skipped (disconnected)"
        );
    }
}
