//! マージ用コンテンツ読み込み。
//!
//! ファイルコンテンツのキャッシュロード（サブツリー一括・単一ファイル）を担当する。
//! ツリーの遅延ロード・展開は `merge_tree_load` に分離。

use crate::app::AppState;
use crate::runtime::TuiRuntime;
use crate::ui::dialog::{DialogState, ProgressDialog, ProgressPhase};

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
    );

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

/// 左側ファイルの読み込み（統一 API 経由）
fn load_left_files(state: &mut AppState, runtime: &mut TuiRuntime, file_paths: &[String]) {
    if state.left_source.is_local() {
        // ローカルは個別読み込み（プログレス表示付き）
        for (i, path) in file_paths.iter().enumerate() {
            if i % 10 == 0 {
                if let DialogState::Progress(ref mut progress) = state.dialog {
                    progress.current = i;
                    progress.current_path = Some(path.clone());
                }
            }
            if !state.left_cache.contains_key(path) {
                match runtime.read_file(&state.left_source, path) {
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
    } else {
        // リモートはバッチ読み込み
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

            match runtime.read_files_batch(&state.left_source, &left_paths) {
                Ok(batch_result) => {
                    for (path, content) in batch_result {
                        state.left_cache.insert(path.clone(), content);
                        state.error_paths.remove(&path);
                    }
                }
                Err(e) => {
                    tracing::warn!("Left batch remote read failed: {}", e);
                    if crate::error::is_connection_error(&e) {
                        state.is_connected = false;
                        runtime.disconnect_if_remote(&state.left_source);
                        state.status_message =
                            format!("Left connection lost: {} | Press 'c' to reconnect", e);
                        state.dialog = DialogState::None;
                    }
                }
            }
        }
    }
}

/// 右側ファイルのバッチ読み込み（統一 API 経由）
fn load_right_files(state: &mut AppState, runtime: &mut TuiRuntime, file_paths: &[String]) {
    let uncached_paths: Vec<String> = file_paths
        .iter()
        .filter(|p| !state.right_cache.contains_key(p))
        .cloned()
        .collect();

    if uncached_paths.is_empty() {
        return;
    }

    if !runtime.is_side_available(&state.right_source) {
        return;
    }

    if let DialogState::Progress(ref mut progress) = state.dialog {
        progress.phase = ProgressPhase::LoadingRemote;
        progress.current = 0;
        progress.total = Some(uncached_paths.len());
    }

    match runtime.read_files_batch(&state.right_source, &uncached_paths) {
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
                runtime.disconnect_if_remote(&state.right_source);
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
        // 左側キャッシュ（統一 API 経由）
        if !state.left_cache.contains_key(path) {
            match runtime.read_file(&state.left_source, path) {
                Ok(content) => {
                    state.left_cache.insert(path.clone(), content);
                    state.error_paths.remove(path);
                }
                Err(e) => {
                    tracing::debug!("Left file read skipped: {} - {}", path, e);
                    if state.left_source.is_remote() && crate::error::is_connection_error(&e) {
                        state.is_connected = false;
                        runtime.disconnect_if_remote(&state.left_source);
                        state.status_message =
                            format!("Left connection lost: {} | Press 'c' to reconnect", e);
                    } else {
                        state.status_message = format!("Left read failed: {} - {}", path, e);
                    }
                }
            }
        }

        // 右側キャッシュ（リモートまたはローカル）
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

    match runtime.read_file(&ref_source, path) {
        Ok(content) => {
            state.ref_cache.insert(path.to_string(), content);
        }
        Err(e) => {
            tracing::debug!("Ref file read skipped: {} - {}", path, e);
        }
    }
}

/// 右側の単一ファイルコンテンツをロードする（統一 API 経由）
fn load_right_file_content(state: &mut AppState, runtime: &mut TuiRuntime, path: &str) {
    if state.right_cache.contains_key(path) {
        return;
    }

    if !runtime.is_side_available(&state.right_source) {
        tracing::warn!(
            path = %path,
            side = %state.right_source.display_name(),
            "Right read skipped (unavailable)"
        );
        return;
    }

    match runtime.read_file(&state.right_source, path) {
        Ok(content) => {
            state.right_cache.insert(path.to_string(), content);
            state.error_paths.remove(path);
        }
        Err(e) => {
            tracing::warn!("Right file read failed: {} - {}", path, e);
            if crate::error::is_connection_error(&e) {
                state.is_connected = false;
                runtime.disconnect_if_remote(&state.right_source);
                state.status_message = format!("Connection lost: {} | Press 'c' to reconnect", e);
            } else {
                state.status_message = format!("Right read failed: {} - {}", path, e);
            }
        }
    }
}
