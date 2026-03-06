//! マージ実行・ファイルコンテンツ読み込み。

use crate::app::AppState;
use crate::backup;
use crate::diff::engine::HunkDirection;
use crate::merge::executor::{self, MergeDirection};
use crate::merge::optimistic_lock::{self, MtimeConflict};
use crate::runtime::TuiRuntime;
use crate::ui::dialog::{
    BatchConfirmDialog, ConfirmDialog, DialogState, MtimeWarningDialog, MtimeWarningMergeContext,
    ProgressDialog, ProgressPhase,
};

/// 単一ファイルの楽観的ロックチェック。
///
/// マージ先のファイルの mtime が diff 取得時から変更されていないかチェック。
/// 衝突がある場合は MtimeWarningDialog を表示し `true` を返す。
pub fn check_mtime_conflict_single(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    path: &str,
    direction: MergeDirection,
) -> bool {
    match direction {
        MergeDirection::LeftToRight => {
            // リモート側のmtimeをチェック
            if !state.is_connected {
                return false;
            }
            let expected = state
                .right_tree
                .find_node(std::path::Path::new(path))
                .and_then(|n| n.mtime);

            match runtime.stat_remote_files(&state.server_name, &[path.to_string()]) {
                Ok(results) => {
                    let actual = results.first().and_then(|(_, dt)| *dt);
                    if let Some(conflict) = optimistic_lock::check_mtime(path, expected, actual) {
                        show_mtime_warning(state, vec![conflict], direction, Some(path));
                        return true;
                    }
                }
                Err(e) => {
                    tracing::warn!("mtime check failed (continuing): {}", e);
                }
            }
        }
        MergeDirection::RightToLeft => {
            // ローカル側のmtimeをチェック
            let expected = state
                .left_tree
                .find_node(std::path::Path::new(path))
                .and_then(|n| n.mtime);
            let actual = optimistic_lock::stat_local_file(&state.left_tree.root, path);

            if let Some(conflict) = optimistic_lock::check_mtime(path, expected, actual) {
                show_mtime_warning(state, vec![conflict], direction, Some(path));
                return true;
            }
        }
    }
    false
}

fn show_mtime_warning(
    state: &mut AppState,
    conflicts: Vec<MtimeConflict>,
    direction: MergeDirection,
    path: Option<&str>,
) {
    let merge_context = match path {
        Some(p) => MtimeWarningMergeContext::Single {
            path: p.to_string(),
            direction,
        },
        None => MtimeWarningMergeContext::Batch { direction },
    };
    state.dialog = DialogState::MtimeWarning(MtimeWarningDialog {
        conflicts,
        merge_context,
    });
}

/// diff viewer からの書き込み時の mtime チェック（w キー / HunkMergePreview）。
///
/// - `hunk_direction` が `Some` なら HunkMerge コンテキスト
/// - `None` なら Write コンテキスト（w キーで両側書き込み）
///
/// 衝突があれば `MtimeWarningDialog` を表示して `true` を返す。
pub fn check_mtime_for_write(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    hunk_direction: Option<crate::diff::engine::HunkDirection>,
) -> bool {
    let path = match &state.selected_path {
        Some(p) => p.clone(),
        None => return false,
    };

    let mut conflicts = Vec::new();

    // ローカル側の mtime チェック
    let local_expected = state
        .left_tree
        .find_node(std::path::Path::new(&path))
        .and_then(|n| n.mtime);
    let local_actual = optimistic_lock::stat_local_file(&state.left_tree.root, &path);
    if let Some(c) = optimistic_lock::check_mtime(&path, local_expected, local_actual) {
        conflicts.push(c);
    }

    // リモート側の mtime チェック
    if state.is_connected {
        let remote_expected = state
            .right_tree
            .find_node(std::path::Path::new(&path))
            .and_then(|n| n.mtime);
        match runtime.stat_remote_files(&state.server_name, std::slice::from_ref(&path)) {
            Ok(results) => {
                let remote_actual = results.first().and_then(|(_, dt)| *dt);
                if let Some(c) = optimistic_lock::check_mtime(&path, remote_expected, remote_actual)
                {
                    conflicts.push(c);
                }
            }
            Err(e) => {
                tracing::warn!("mtime check failed (continuing): {}", e);
            }
        }
    }

    if conflicts.is_empty() {
        return false;
    }

    let merge_context = match hunk_direction {
        Some(dir) => MtimeWarningMergeContext::HunkMerge { direction: dir },
        None => MtimeWarningMergeContext::Write,
    };
    state.dialog = DialogState::MtimeWarning(MtimeWarningDialog {
        conflicts,
        merge_context,
    });
    true
}

/// マージを実行する
pub fn execute_merge(state: &mut AppState, runtime: &mut TuiRuntime, confirm: &ConfirmDialog) {
    use crate::diff::engine::DiffResult;

    let path = &confirm.file_path;
    let direction = confirm.direction;

    // シンボリックリンクの場合は専用のマージ処理
    if let Some(DiffResult::SymlinkDiff { .. }) = &state.current_diff {
        super::symlink_merge::execute_symlink_merge(state, runtime, path, direction);
        return;
    }

    // バイナリファイルのマージは未対応（バイト列I/Oが必要）
    if let Some(DiffResult::Binary { .. }) = &state.current_diff {
        state.status_message = format!("{}: binary file merge is not yet supported", path);
        return;
    }

    match direction {
        MergeDirection::LeftToRight => {
            let content = match state.left_cache.get(path) {
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

            // バックアップ（リモート側）
            if runtime.config.backup.enabled {
                if let Err(e) =
                    runtime.create_remote_backups(&state.server_name, std::slice::from_ref(path))
                {
                    tracing::warn!("Remote backup failed (continuing): {}", e);
                }
            }

            match runtime.write_remote_file(&state.server_name, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    let left_name = state.left_source.display_name();
                    state.status_message =
                        format!("{}: {} -> {} merged", path, left_name, state.server_name);
                }
                Err(e) => {
                    state.status_message = format!("Merge failed: {}", e);
                }
            }
        }
        MergeDirection::RightToLeft => {
            let content = match state.right_cache.get(path) {
                Some(c) => c.clone(),
                None => {
                    state.status_message = format!("{}: remote content not loaded", path);
                    return;
                }
            };

            // バックアップ（ローカル側）
            if runtime.config.backup.enabled {
                let backup_dir = state.left_tree.root.join(backup::BACKUP_DIR_NAME);
                if let Err(e) =
                    backup::create_local_backup(&state.left_tree.root, path, &backup_dir)
                {
                    tracing::warn!("Local backup failed (continuing): {}", e);
                }
            }

            let local_root = state.left_tree.root.clone();
            match executor::write_local_file(&local_root, path, &content) {
                Ok(()) => {
                    state.update_badge_after_merge(path, &content, direction);
                    let left_name = state.left_source.display_name();
                    state.status_message =
                        format!("{}: {} -> {} merged", path, state.server_name, left_name);
                }
                Err(e) => {
                    state.status_message = format!("Merge failed: {}", e);
                }
            }
        }
    }
}

/// バッチマージを実行する（ディレクトリ選択時）
///
/// `Badge::Unchecked` のファイルはマージ前にキャッシュの内容を比較し、
/// 実際に差分があるもののみマージする。同一内容ならスキップする。
pub fn execute_batch_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    batch: &BatchConfirmDialog,
) {
    let direction = batch.direction;
    let mut success_count = 0usize;
    let mut fail_count = 0usize;

    // Unchecked ファイルの差分チェック: 同一内容のファイルを除外する
    let (files, skipped_equal) =
        filter_unchecked_equal(&batch.files, &state.left_cache, &state.right_cache);

    let file_count = files.len();

    // 全ファイルが同一だった場合は早期リターン
    if file_count == 0 {
        state.status_message = format!(
            "Batch merge skipped: all {} file(s) are identical",
            skipped_equal
        );
        return;
    }

    // バックアップ（マージ前に一括実行）
    if runtime.config.backup.enabled {
        let file_paths: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
        match direction {
            MergeDirection::LeftToRight => {
                // リモート側バックアップ（バッチ）
                if state.is_connected {
                    if let Err(e) = runtime.create_remote_backups(&state.server_name, &file_paths) {
                        tracing::warn!("Remote batch backup failed (continuing): {}", e);
                    }
                }
            }
            MergeDirection::RightToLeft => {
                // ローカル側バックアップ
                let backup_dir = state.left_tree.root.join(backup::BACKUP_DIR_NAME);
                for path in &file_paths {
                    if let Err(e) =
                        backup::create_local_backup(&state.left_tree.root, path, &backup_dir)
                    {
                        tracing::warn!("Local backup failed for {}: {}", path, e);
                    }
                }
            }
        }
    }

    // プログレスダイアログを表示
    let mut progress = ProgressDialog::new(ProgressPhase::Merging, "", false);
    progress.total = Some(file_count);
    state.dialog = DialogState::Progress(progress);

    for (i, (path, _badge)) in files.iter().enumerate() {
        // ダイアログの進捗を更新
        if let DialogState::Progress(ref mut progress) = state.dialog {
            progress.current = i + 1;
            progress.current_path = Some(path.clone());
        }

        match direction {
            MergeDirection::LeftToRight => {
                let content = match state.left_cache.get(path) {
                    Some(c) => c.clone(),
                    None => {
                        let local_root = &state.left_tree.root;
                        match executor::read_local_file(local_root, path) {
                            Ok(c) => {
                                state.left_cache.insert(path.clone(), c.clone());
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
                        state.sync_cache_after_merge(path, &content, direction);
                        success_count += 1;
                    }
                    Err(e) => {
                        if crate::error::is_connection_error(&e) {
                            state.is_connected = false;
                            state.status_message = format!(
                                "Connection lost during merge: {} succeeded/{} failed",
                                success_count,
                                fail_count + 1
                            );
                            return;
                        }
                        tracing::warn!("Batch merge failed: {} - {}", path, e);
                        fail_count += 1;
                    }
                }
            }
            MergeDirection::RightToLeft => {
                let content = match state.right_cache.get(path) {
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
                                state.right_cache.insert(path.clone(), c.clone());
                                c
                            }
                            Err(e) => {
                                if crate::error::is_connection_error(&e) {
                                    state.is_connected = false;
                                    state.status_message = format!(
                                        "Connection lost during merge: {} succeeded/{} failed",
                                        success_count,
                                        fail_count + 1
                                    );
                                    return;
                                }
                                fail_count += 1;
                                continue;
                            }
                        }
                    }
                };

                let local_root = state.left_tree.root.clone();
                match executor::write_local_file(&local_root, path, &content) {
                    Ok(()) => {
                        state.sync_cache_after_merge(path, &content, direction);
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

    // プログレスダイアログを閉じる
    state.dialog = DialogState::None;

    // バッジ再計算（バッチ全体で1回だけ）
    if state.selected_path.is_some() {
        state.select_file();
    }
    state.rebuild_flat_nodes();

    let left_name = state.left_source.display_name();
    let dir_str = match direction {
        MergeDirection::LeftToRight => format!("{} -> {}", left_name, state.server_name),
        MergeDirection::RightToLeft => format!("{} -> {}", state.server_name, left_name),
    };

    let skip_suffix = if skipped_equal > 0 {
        format!(", {} identical skipped", skipped_equal)
    } else {
        String::new()
    };

    if fail_count == 0 {
        state.status_message = format!(
            "Batch merge complete: {} files merged ({}){}",
            success_count, dir_str, skip_suffix
        );
    } else {
        state.status_message = format!(
            "Batch merge complete: {} succeeded/{} failed ({}){}",
            success_count, fail_count, dir_str, skip_suffix
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
        // バックアップ
        if runtime.config.backup.enabled {
            match direction {
                HunkDirection::RightToLeft => {
                    let backup_dir = state.left_tree.root.join(backup::BACKUP_DIR_NAME);
                    if let Err(e) =
                        backup::create_local_backup(&state.left_tree.root, &path, &backup_dir)
                    {
                        tracing::warn!("Local backup failed (continuing): {}", e);
                    }
                }
                HunkDirection::LeftToRight => {
                    if state.is_connected {
                        if let Err(e) = runtime
                            .create_remote_backups(&state.server_name, std::slice::from_ref(&path))
                        {
                            tracing::warn!("Remote backup failed (continuing): {}", e);
                        }
                    }
                }
            }
        }

        match direction {
            HunkDirection::RightToLeft => {
                let content = state.left_cache.get(&path).cloned().unwrap_or_default();
                let local_root = state.left_tree.root.clone();
                match executor::write_local_file(&local_root, &path, &content) {
                    Ok(()) => {
                        let left_name = state.left_source.display_name();
                        state.status_message = format!(
                            "Hunk merged: {} -> {} ({}) | {} hunks left",
                            state.server_name,
                            left_name,
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
                let content = state.right_cache.get(&path).cloned().unwrap_or_default();
                match runtime.write_remote_file(&state.server_name, &path, &content) {
                    Ok(()) => {
                        let left_name = state.left_source.display_name();
                        state.status_message = format!(
                            "Hunk merged: {} -> {} ({}) | {} hunks left",
                            left_name,
                            state.server_name,
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

        // バックアップ（両側）
        if runtime.config.backup.enabled {
            let backup_dir = state.left_tree.root.join(backup::BACKUP_DIR_NAME);
            if let Err(e) = backup::create_local_backup(&state.left_tree.root, &path, &backup_dir) {
                tracing::warn!("Local backup failed (continuing): {}", e);
            }
            if state.is_connected {
                if let Err(e) =
                    runtime.create_remote_backups(&state.server_name, std::slice::from_ref(&path))
                {
                    tracing::warn!("Remote backup failed (continuing): {}", e);
                }
            }
        }

        if let Some(local_content) = state.left_cache.get(&path) {
            let local_root = state.left_tree.root.clone();
            if let Err(e) = executor::write_local_file(&local_root, &path, local_content) {
                state.status_message = format!("Local write failed: {}", e);
                return;
            }
        }

        if state.is_connected {
            if let Some(remote_content) = state.right_cache.get(&path).cloned() {
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

/// リモートディレクトリの遅延読み込み（右側ツリー用、従来互換）
pub fn load_remote_children(state: &mut AppState, runtime: &mut TuiRuntime, rel_path: &str) {
    let server_name = state.server_name.clone();
    load_remote_children_to(state, runtime, rel_path, &server_name, false);
}

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

    let client = match runtime.ssh_clients.get_mut(server_name) {
        Some(c) => c,
        None => return,
    };

    let tree = if is_left {
        &mut state.left_tree
    } else {
        &mut state.right_tree
    };

    match runtime.rt.block_on(client.list_dir(&full_path, &exclude)) {
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

        // ローカルツリーのサブディレクトリを収集
        let mut sub_dirs = Vec::new();
        if let Some(node) = state.left_tree.find_node(std::path::Path::new(&path)) {
            if let Some(children) = &node.children {
                for child in children {
                    if child.is_dir() {
                        sub_dirs.push(format!("{}/{}", path, child.name));
                    }
                }
            }
        }

        // リモートツリーのサブディレクトリも収集（ローカルにないものも含む）
        if let Some(node) = state.right_tree.find_node(std::path::Path::new(&path)) {
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
    // マージ前に古いキャッシュが残っていると、第三者の変更が反映されず
    // 差分なしと誤判定される（load_file_content と同じ方針）。
    state.invalidate_cache_for_paths(&file_paths);

    // ── ローカルファイルを個別に読み込み ──
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

    // ── リモートファイルをバッチ読み込み（1チャネルで全ファイル） ──
    let remote_paths: Vec<String> = file_paths
        .iter()
        .filter(|p| !state.right_cache.contains_key(*p))
        .cloned()
        .collect();

    if !remote_paths.is_empty() {
        if !state.is_connected && runtime.check_connection(&state.server_name) {
            tracing::info!("SSH connection recovered during subtree load");
            state.is_connected = true;
        }

        if state.is_connected {
            if let DialogState::Progress(ref mut progress) = state.dialog {
                progress.phase = ProgressPhase::LoadingRemote;
                progress.current = 0;
                progress.total = Some(remote_paths.len());
            }

            match runtime.read_remote_files_batch(&state.server_name, &remote_paths) {
                Ok(batch_result) => {
                    for (path, content) in batch_result {
                        // 空文字列のファイルも有効（0バイトファイル）
                        state.right_cache.insert(path.clone(), content);
                        state.error_paths.remove(&path);
                    }
                }
                Err(e) => {
                    tracing::warn!("Batch remote read failed: {}", e);
                    if crate::error::is_connection_error(&e) {
                        state.is_connected = false;
                        state.status_message =
                            format!("Connection lost: {} | Press 'c' to reconnect", e);
                        state.dialog = DialogState::None;
                        return;
                    }
                }
            }
        }
    }

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

/// ファイル選択時にコンテンツをロードする。
///
/// 未保存の変更がない場合はキャッシュを無効化して毎回再取得する。
/// これにより、マージ後に第三者がリモートファイルを変更した場合でも
/// 最新の内容が表示される。
pub fn load_file_content(state: &mut AppState, runtime: &mut TuiRuntime) {
    let node = match state.flat_nodes.get(state.tree_cursor) {
        Some(n) if !n.is_dir => n.clone(),
        _ => return,
    };

    // シンボリックリンクはツリーノードから直接比較するため、
    // テキスト/バイナリキャッシュへの読み込みは不要
    if node.is_symlink {
        return;
    }

    let path = &node.path;

    // 未保存変更がなければキャッシュを無効化して最新を取得
    if !state.has_unsaved_changes() {
        state.invalidate_cache_for_paths(std::slice::from_ref(path));
    }

    // ローカルキャッシュ
    if !state.left_cache.contains_key(path) {
        let local_root = &state.left_tree.root;
        match executor::read_local_file(local_root, path) {
            Ok(content) => {
                state.left_cache.insert(path.clone(), content);
                state.error_paths.remove(path);
            }
            Err(e) => {
                tracing::debug!("Local file read skipped: {} - {}", path, e);
                state.status_message = format!("Local read failed: {} - {}", path, e);
            }
        }
    }

    // リモートキャッシュ
    if !state.right_cache.contains_key(path) {
        if !state.is_connected {
            // 切断状態だが、実際に接続が回復してないか確認
            if runtime.check_connection(&state.server_name) {
                tracing::warn!("SSH connection recovered, resuming remote operations");
                state.is_connected = true;
            }
        }

        if state.is_connected {
            match runtime.read_remote_file(&state.server_name, path) {
                Ok(content) => {
                    state.right_cache.insert(path.clone(), content);
                    state.error_paths.remove(path);
                }
                Err(e) => {
                    tracing::warn!("Remote file read failed: {} - {}", path, e);
                    if crate::error::is_connection_error(&e) {
                        state.is_connected = false;
                        state.status_message =
                            format!("Connection lost: {} | Press 'c' to reconnect", e);
                    } else {
                        state.status_message = format!("Remote read failed: {} - {}", path, e);
                    }
                }
            }
        } else {
            tracing::warn!(
                "Remote read skipped (disconnected): {} | has_client={}",
                path,
                runtime.has_client(&state.server_name)
            );
        }
    }

    // 両方とも読み込めなかった場合のみエラー扱い
    if !state.left_cache.contains_key(path) && !state.right_cache.contains_key(path) {
        state.error_paths.insert(path.clone());
    }

    state.rebuild_flat_nodes();
}

/// バッチマージ対象から同一内容の Unchecked ファイルを除外する。
///
/// `Badge::Unchecked` のファイルについて、ローカル・リモート両方のキャッシュが
/// 存在し内容が同一であればスキップする。
/// 戻り値は `(フィルタ済みファイル一覧, スキップ数)`.
pub fn filter_unchecked_equal(
    files: &[(String, crate::app::Badge)],
    local_cache: &std::collections::HashMap<String, String>,
    remote_cache: &std::collections::HashMap<String, String>,
) -> (Vec<(String, crate::app::Badge)>, usize) {
    let mut skipped = 0usize;
    let filtered = files
        .iter()
        .filter(|(path, badge)| {
            if *badge != crate::app::Badge::Unchecked {
                return true;
            }
            match (local_cache.get(path), remote_cache.get(path)) {
                (Some(local), Some(remote)) => {
                    if local == remote {
                        skipped += 1;
                        false
                    } else {
                        true
                    }
                }
                _ => true,
            }
        })
        .cloned()
        .collect();
    (filtered, skipped)
}

/// パスがローカルまたはリモートツリーでシンボリックリンクかどうかを判定する
fn is_symlink_in_tree(state: &AppState, path: &str) -> bool {
    let local_symlink = state
        .left_tree
        .find_node(path)
        .is_some_and(|n| n.is_symlink());
    let remote_symlink = state
        .right_tree
        .find_node(path)
        .is_some_and(|n| n.is_symlink());
    local_symlink || remote_symlink
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Badge;
    use std::collections::HashMap;

    #[test]
    fn test_filter_unchecked_equal_skips_identical() {
        let files = vec![
            ("a.rs".to_string(), Badge::Unchecked),
            ("b.rs".to_string(), Badge::Modified),
        ];
        let mut local = HashMap::new();
        let mut remote = HashMap::new();
        local.insert("a.rs".to_string(), "same".to_string());
        remote.insert("a.rs".to_string(), "same".to_string());

        let (filtered, skipped) = filter_unchecked_equal(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "b.rs");
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_filter_unchecked_equal_keeps_different() {
        let files = vec![("a.rs".to_string(), Badge::Unchecked)];
        let mut local = HashMap::new();
        let mut remote = HashMap::new();
        local.insert("a.rs".to_string(), "old".to_string());
        remote.insert("a.rs".to_string(), "new".to_string());

        let (filtered, skipped) = filter_unchecked_equal(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_unchecked_equal_keeps_missing_cache() {
        // 片方しかキャッシュにない場合 → スキップしない（安全側に倒す）
        let files = vec![("a.rs".to_string(), Badge::Unchecked)];
        let mut local = HashMap::new();
        let remote = HashMap::new();
        local.insert("a.rs".to_string(), "content".to_string());

        let (filtered, skipped) = filter_unchecked_equal(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_unchecked_equal_preserves_non_unchecked() {
        // Modified / LocalOnly / RemoteOnly は無条件で通す
        let files = vec![
            ("a.rs".to_string(), Badge::Modified),
            ("b.rs".to_string(), Badge::LeftOnly),
            ("c.rs".to_string(), Badge::RightOnly),
        ];
        let local = HashMap::new();
        let remote = HashMap::new();

        let (filtered, skipped) = filter_unchecked_equal(&files, &local, &remote);
        assert_eq!(filtered.len(), 3);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_unchecked_equal_all_identical() {
        let files = vec![
            ("a.rs".to_string(), Badge::Unchecked),
            ("b.rs".to_string(), Badge::Unchecked),
        ];
        let mut local = HashMap::new();
        let mut remote = HashMap::new();
        local.insert("a.rs".to_string(), "x".to_string());
        remote.insert("a.rs".to_string(), "x".to_string());
        local.insert("b.rs".to_string(), "y".to_string());
        remote.insert("b.rs".to_string(), "y".to_string());

        let (filtered, skipped) = filter_unchecked_equal(&files, &local, &remote);
        assert_eq!(filtered.len(), 0);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn test_filter_unchecked_equal_mixed() {
        let files = vec![
            ("equal.rs".to_string(), Badge::Unchecked),
            ("diff.rs".to_string(), Badge::Unchecked),
            ("known.rs".to_string(), Badge::Modified),
            ("no_cache.rs".to_string(), Badge::Unchecked),
        ];
        let mut local = HashMap::new();
        let mut remote = HashMap::new();
        local.insert("equal.rs".to_string(), "same".to_string());
        remote.insert("equal.rs".to_string(), "same".to_string());
        local.insert("diff.rs".to_string(), "aaa".to_string());
        remote.insert("diff.rs".to_string(), "bbb".to_string());
        // no_cache.rs はキャッシュなし

        let (filtered, skipped) = filter_unchecked_equal(&files, &local, &remote);
        assert_eq!(filtered.len(), 3); // diff.rs, known.rs, no_cache.rs
        assert_eq!(skipped, 1); // equal.rs
        assert_eq!(filtered[0].0, "diff.rs");
        assert_eq!(filtered[1].0, "known.rs");
        assert_eq!(filtered[2].0, "no_cache.rs");
    }
}
