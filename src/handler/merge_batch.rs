//! バッチマージ実行ロジック。
//!
//! ディレクトリ選択時のバッチマージと、
//! Unchecked ファイルの同一内容フィルタリング。

use crate::app::AppState;
use crate::merge::executor::MergeDirection;
use crate::runtime::TuiRuntime;
use crate::service::merge::{determine_merge_action, MergeAction};
use crate::ui::dialog::{BatchConfirmDialog, DialogState, ProgressDialog, ProgressPhase};

use super::merge_file_io::{
    backup_left, backup_right, read_left_file, write_left_file, write_right_file,
};

/// バッチマージを実行する（ディレクトリ選択時）
///
/// `Badge::Unchecked` / `Badge::LeftOnly` / `Badge::RightOnly` のファイルは
/// マージ前にキャッシュの内容を比較し、実際に差分があるもののみマージする。
/// 同一内容ならスキップする（ツリーとキャッシュの不整合による誤マージを防止）。
pub fn execute_batch_merge(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    batch: &BatchConfirmDialog,
) {
    let direction = batch.direction;
    let mut success_count = 0usize;
    let mut fail_count = 0usize;

    // 同一内容ファイルの除外: Unchecked/LeftOnly/RightOnly をキャッシュ比較してスキップ
    let (files, skipped_equal) =
        filter_identical_files(&batch.files, &state.left_cache, &state.right_cache);

    let file_count = files.len();

    tracing::info!(
        "Batch merge started: files={}, direction={:?}, skipped_identical={}",
        file_count,
        direction,
        skipped_equal
    );

    // 全ファイルが同一だった場合は早期リターン
    if file_count == 0 {
        state.status_message = format!(
            "Batch merge skipped: all {} file(s) are identical",
            skipped_equal
        );
        return;
    }

    // symlink アクションを事前計算（borrow checker 対策: ツリーの immutable borrow をループ前に終了）
    let symlink_actions: Vec<(String, MergeAction)> = {
        let (source_tree, target_tree) = match direction {
            MergeDirection::LeftToRight => (&state.left_tree, &state.right_tree),
            MergeDirection::RightToLeft => (&state.right_tree, &state.left_tree),
        };
        files
            .iter()
            .map(|(p, _)| {
                let action = determine_merge_action(source_tree, target_tree, p);
                (p.clone(), action)
            })
            .collect()
    };

    // source_side / target_side を Clone で取得（borrow 分離）
    let source_side = match direction {
        MergeDirection::LeftToRight => state.left_source.clone(),
        MergeDirection::RightToLeft => state.right_source.clone(),
    };
    let target_side = match direction {
        MergeDirection::LeftToRight => state.right_source.clone(),
        MergeDirection::RightToLeft => state.left_source.clone(),
    };

    // セッションIDを1度だけ生成（全ファイルで共有）
    let session_id = crate::backup::backup_timestamp();

    // バックアップ（マージ前に一括実行）
    if runtime.core.config.backup.enabled {
        let backup_paths = collect_backup_paths(&symlink_actions);
        if !backup_paths.is_empty() {
            match direction {
                MergeDirection::LeftToRight => {
                    if runtime.is_side_available(&state.right_source) {
                        backup_right(state, runtime, &backup_paths, &session_id);
                    }
                }
                MergeDirection::RightToLeft => {
                    backup_left(state, runtime, &backup_paths, &session_id);
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

        // symlink 判定（事前計算結果をインデックスで参照 — files と同じ順序）
        let action = symlink_actions[i].1.clone();

        match action {
            MergeAction::CreateSymlink { .. } | MergeAction::ReplaceSymlinkWithFile => {
                let params = super::symlink_merge::SymlinkMergeParams {
                    path,
                    direction,
                    action,
                    source_side: &source_side,
                    target_side: &target_side,
                    session_id: &session_id,
                };
                let ok = super::symlink_merge::execute_symlink_merge(state, runtime, &params);
                if ok {
                    success_count += 1;
                } else {
                    fail_count += 1;
                }
                continue;
            }
            MergeAction::Normal => {
                // 通常マージ処理へ
            }
        }

        match direction {
            MergeDirection::LeftToRight => {
                let content = match state.left_cache.get(path) {
                    Some(c) => c.clone(),
                    None => match read_left_file(state, runtime, path) {
                        Ok(c) => {
                            state.left_cache.insert(path.clone(), c.clone());
                            c
                        }
                        Err(_) => {
                            fail_count += 1;
                            continue;
                        }
                    },
                };

                if !runtime.is_side_available(&state.right_source) {
                    state.status_message = format!(
                        "SSH disconnected: results so far: {} succeeded/{} failed",
                        success_count, fail_count
                    );
                    return;
                }

                match write_right_file(state, runtime, path, &content) {
                    Ok(()) => {
                        state.sync_cache_after_merge(path, &content, direction);
                        success_count += 1;
                    }
                    Err(e) => {
                        if crate::error::is_connection_error(&e) {
                            state.is_connected = false;
                            runtime.disconnect_if_remote(&state.right_source);
                            tracing::error!(
                                "Connection lost during batch merge: file={}, progress={}/{}, error={}",
                                path, success_count, file_count, e
                            );
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
                        if !runtime.is_side_available(&state.right_source) {
                            state.status_message = format!(
                                "SSH disconnected: results so far: {} succeeded/{} failed",
                                success_count, fail_count
                            );
                            return;
                        }
                        match runtime.read_file(&state.right_source, path) {
                            Ok(c) => {
                                state.right_cache.insert(path.clone(), c.clone());
                                c
                            }
                            Err(e) => {
                                if crate::error::is_connection_error(&e) {
                                    state.is_connected = false;
                                    runtime.disconnect_if_remote(&state.right_source);
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

                match write_left_file(state, runtime, path, &content) {
                    Ok(()) => {
                        state.sync_cache_after_merge(path, &content, direction);
                        success_count += 1;
                    }
                    Err(e) => {
                        if crate::error::is_connection_error(&e) {
                            state.is_connected = false;
                            runtime.disconnect_if_remote(&state.left_source);
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
        }
    }

    // プログレスダイアログを閉じる
    state.dialog = DialogState::None;

    // バッジ再計算（バッチ全体で1回だけ）
    if state.selected_path.is_some() {
        state.select_file();
    }
    // ref_tree の深さ同期（マージしたファイルのディレクトリについて ref 子ノードをロード）
    if state.has_reference() {
        let dirs = collect_merge_dirs(&files);
        for dir in &dirs {
            super::merge_tree_load::load_ref_children(state, runtime, dir);
        }
    }

    state.rebuild_flat_nodes();

    let left = state.left_source.display_name();
    let right = state.right_source.display_name();
    let dir_str = match direction {
        MergeDirection::LeftToRight => format!("{} -> {}", left, right),
        MergeDirection::RightToLeft => format!("{} -> {}", right, left),
    };

    tracing::info!(
        "Batch merge completed: success={}, failed={}, skipped_identical={}",
        success_count,
        fail_count,
        skipped_equal
    );

    state.status_message = format_batch_summary(success_count, fail_count, skipped_equal, &dir_str);
}

/// バッチマージ結果のサマリーメッセージを生成する（純粋関数）。
fn format_batch_summary(
    success_count: usize,
    fail_count: usize,
    skipped_equal: usize,
    dir_str: &str,
) -> String {
    let skip_suffix = if skipped_equal > 0 {
        format!(", {} identical skipped", skipped_equal)
    } else {
        String::new()
    };

    if fail_count == 0 {
        format!(
            "Batch merge complete: {} files merged ({}){}",
            success_count, dir_str, skip_suffix
        )
    } else {
        format!(
            "Batch merge complete: {} succeeded/{} failed ({}){}",
            success_count, fail_count, dir_str, skip_suffix
        )
    }
}

/// symlink 以外のバックアップ対象パスを収集する（純粋関数）。
///
/// symlink ファイルは `execute_symlink_merge` 内で個別バックアップするため除外する。
fn collect_backup_paths(symlink_actions: &[(String, MergeAction)]) -> Vec<String> {
    symlink_actions
        .iter()
        .filter(|(_, action)| matches!(action, MergeAction::Normal))
        .map(|(p, _)| p.clone())
        .collect()
}

/// マージ対象ファイルのディレクトリパスを収集する（ref_tree 同期用）
///
/// ルートディレクトリのファイル（パスに `/` を含まない）は `""` として返す。
/// `load_ref_children("")` はルートノードが既にロード済みのため no-op となる。
fn collect_merge_dirs(files: &[(String, crate::app::Badge)]) -> std::collections::BTreeSet<String> {
    files
        .iter()
        .map(|(path, _)| {
            path.rsplit_once('/')
                .map(|(dir, _)| dir.to_string())
                .unwrap_or_default() // ルートファイルは "" として返す（load_ref_children で no-op）
        })
        .collect()
}

/// バッチマージ対象から同一内容のファイルを除外する。
///
/// `Badge::Unchecked` / `Badge::LeftOnly` / `Badge::RightOnly` のファイルについて、
/// ローカル・リモート両方のキャッシュが存在し内容が同一であればスキップする。
/// LeftOnly/RightOnly はツリーとキャッシュの不整合により誤判定されている可能性があるため、
/// マージ前の安全弁としてキャッシュ比較を行う。
/// 戻り値は `(フィルタ済みファイル一覧, スキップ数)`.
pub fn filter_identical_files(
    files: &[(String, crate::app::Badge)],
    local_cache: &crate::app::cache::BoundedCache<String>,
    remote_cache: &crate::app::cache::BoundedCache<String>,
) -> (Vec<(String, crate::app::Badge)>, usize) {
    use crate::app::Badge;
    let mut skipped = 0usize;
    let filtered = files
        .iter()
        .filter(|(path, badge)| {
            let should_check =
                matches!(badge, Badge::Unchecked | Badge::LeftOnly | Badge::RightOnly);
            if !should_check {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cache::BoundedCache;
    use crate::app::Badge;

    fn make_cache(entries: Vec<(&str, &str)>) -> BoundedCache<String> {
        let mut cache = BoundedCache::new(100);
        for (k, v) in entries {
            cache.insert(k.to_string(), v.to_string());
        }
        cache
    }

    #[test]
    fn test_filter_identical_files_skips_identical() {
        let files = vec![
            ("a.rs".to_string(), Badge::Unchecked),
            ("b.rs".to_string(), Badge::Modified),
        ];
        let local = make_cache(vec![("a.rs", "same")]);
        let remote = make_cache(vec![("a.rs", "same")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "b.rs");
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_filter_identical_files_keeps_different() {
        let files = vec![("a.rs".to_string(), Badge::Unchecked)];
        let local = make_cache(vec![("a.rs", "old")]);
        let remote = make_cache(vec![("a.rs", "new")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_keeps_missing_cache() {
        // 片方しかキャッシュにない場合 → スキップしない（安全側に倒す）
        let files = vec![("a.rs".to_string(), Badge::Unchecked)];
        let local = make_cache(vec![("a.rs", "content")]);
        let remote = BoundedCache::new(100);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_preserves_modified() {
        // Modified は無条件で通す
        let files = vec![("a.rs".to_string(), Badge::Modified)];
        let local = make_cache(vec![("a.rs", "same")]);
        let remote = make_cache(vec![("a.rs", "same")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_preserves_equal() {
        // Equal は無条件で通す（フィルタ対象外）
        let files = vec![("a.rs".to_string(), Badge::Equal)];
        let local = make_cache(vec![("a.rs", "same")]);
        let remote = make_cache(vec![("a.rs", "same")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_leftonly_no_cache_passes() {
        // LeftOnly でキャッシュなし → 通常通り通過（安全側）
        let files = vec![("a.rs".to_string(), Badge::LeftOnly)];
        let local = BoundedCache::new(100);
        let remote = BoundedCache::new(100);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_rightonly_no_cache_passes() {
        // RightOnly でキャッシュなし → 通常通り通過
        let files = vec![("a.rs".to_string(), Badge::RightOnly)];
        let local = BoundedCache::new(100);
        let remote = BoundedCache::new(100);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_all_identical() {
        let files = vec![
            ("a.rs".to_string(), Badge::Unchecked),
            ("b.rs".to_string(), Badge::Unchecked),
        ];
        let local = make_cache(vec![("a.rs", "x"), ("b.rs", "y")]);
        let remote = make_cache(vec![("a.rs", "x"), ("b.rs", "y")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 0);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn test_filter_identical_files_mixed() {
        let files = vec![
            ("equal.rs".to_string(), Badge::Unchecked),
            ("diff.rs".to_string(), Badge::Unchecked),
            ("known.rs".to_string(), Badge::Modified),
            ("no_cache.rs".to_string(), Badge::Unchecked),
        ];
        let local = make_cache(vec![("equal.rs", "same"), ("diff.rs", "aaa")]);
        let remote = make_cache(vec![("equal.rs", "same"), ("diff.rs", "bbb")]);
        // no_cache.rs はキャッシュなし

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 3); // diff.rs, known.rs, no_cache.rs
        assert_eq!(skipped, 1); // equal.rs
        assert_eq!(filtered[0].0, "diff.rs");
        assert_eq!(filtered[1].0, "known.rs");
        assert_eq!(filtered[2].0, "no_cache.rs");
    }

    #[test]
    fn test_collect_merge_dirs_nested() {
        let files = vec![("src/app/foo.rs".to_string(), Badge::Modified)];
        let dirs = collect_merge_dirs(&files);
        assert_eq!(dirs.len(), 1);
        assert!(dirs.contains("src/app"));
    }

    #[test]
    fn test_collect_merge_dirs_root_files() {
        let files = vec![("README.md".to_string(), Badge::Modified)];
        let dirs = collect_merge_dirs(&files);
        assert_eq!(dirs.len(), 1);
        assert!(dirs.contains(""));
    }

    #[test]
    fn test_collect_merge_dirs_mixed() {
        let files = vec![
            ("config.toml".to_string(), Badge::Modified),
            ("src/main.rs".to_string(), Badge::LeftOnly),
        ];
        let dirs = collect_merge_dirs(&files);
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(""));
        assert!(dirs.contains("src"));
    }

    #[test]
    fn test_collect_merge_dirs_dedup() {
        let files = vec![
            ("src/a.rs".to_string(), Badge::Modified),
            ("src/b.rs".to_string(), Badge::Modified),
            ("src/c.rs".to_string(), Badge::LeftOnly),
        ];
        let dirs = collect_merge_dirs(&files);
        assert_eq!(dirs.len(), 1);
        assert!(dirs.contains("src"));
    }

    #[test]
    fn test_collect_merge_dirs_empty() {
        let files: Vec<(String, Badge)> = vec![];
        let dirs = collect_merge_dirs(&files);
        assert!(dirs.is_empty());
    }

    #[test]
    fn test_filter_identical_files_empty_files() {
        // 空のファイルリスト → (空vec, 0)
        let files: Vec<(String, Badge)> = vec![];
        let local = BoundedCache::new(100);
        let remote = BoundedCache::new(100);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert!(filtered.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_empty_content_identical() {
        // 空文字列同士もキャッシュ比較で同一扱い → スキップされる
        let files = vec![("empty.rs".to_string(), Badge::Unchecked)];
        let local = make_cache(vec![("empty.rs", "")]);
        let remote = make_cache(vec![("empty.rs", "")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert!(filtered.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_collect_merge_dirs_deeply_nested() {
        // 3階層以上のネスト → 最後の / までがディレクトリ
        let files = vec![("a/b/c/d/file.rs".to_string(), Badge::Modified)];
        let dirs = collect_merge_dirs(&files);
        assert_eq!(dirs.len(), 1);
        assert!(dirs.contains("a/b/c/d"));
    }

    // ── format_batch_summary ──

    #[test]
    fn test_format_batch_summary_all_success() {
        let msg = format_batch_summary(5, 0, 0, "local -> remote");
        assert_eq!(
            msg,
            "Batch merge complete: 5 files merged (local -> remote)"
        );
    }

    #[test]
    fn test_format_batch_summary_with_failures() {
        let msg = format_batch_summary(3, 2, 0, "local -> remote");
        assert_eq!(
            msg,
            "Batch merge complete: 3 succeeded/2 failed (local -> remote)"
        );
    }

    #[test]
    fn test_format_batch_summary_with_skipped() {
        let msg = format_batch_summary(5, 0, 3, "remote -> local");
        assert_eq!(
            msg,
            "Batch merge complete: 5 files merged (remote -> local), 3 identical skipped"
        );
    }

    #[test]
    fn test_format_batch_summary_with_failures_and_skipped() {
        let msg = format_batch_summary(2, 1, 4, "local -> remote");
        assert_eq!(
            msg,
            "Batch merge complete: 2 succeeded/1 failed (local -> remote), 4 identical skipped"
        );
    }

    // ── collect_backup_paths ──

    #[test]
    fn test_collect_backup_paths_filters_symlinks() {
        let actions = vec![
            ("normal.rs".to_string(), MergeAction::Normal),
            (
                "link.rs".to_string(),
                MergeAction::CreateSymlink {
                    link_target: "/target".to_string(),
                    target_exists: false,
                },
            ),
            (
                "replace.rs".to_string(),
                MergeAction::ReplaceSymlinkWithFile,
            ),
            ("normal2.rs".to_string(), MergeAction::Normal),
        ];
        let paths = collect_backup_paths(&actions);
        assert_eq!(paths, vec!["normal.rs", "normal2.rs"]);
    }

    #[test]
    fn test_collect_backup_paths_empty() {
        let actions: Vec<(String, MergeAction)> = vec![];
        let paths = collect_backup_paths(&actions);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_collect_backup_paths_all_symlinks() {
        let actions = vec![(
            "link.rs".to_string(),
            MergeAction::CreateSymlink {
                link_target: "/target".to_string(),
                target_exists: true,
            },
        )];
        let paths = collect_backup_paths(&actions);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_filter_identical_files_only_equal_badge_skipped() {
        // Equal バッジは Unchecked ではないのでフィルタされない（通過する）
        let files = vec![
            ("a.rs".to_string(), Badge::Equal),
            ("b.rs".to_string(), Badge::Equal),
        ];
        let local = make_cache(vec![("a.rs", "same"), ("b.rs", "same")]);
        let remote = make_cache(vec![("a.rs", "same"), ("b.rs", "same")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        // Equal バッジは無条件通過
        assert_eq!(filtered.len(), 2);
        assert_eq!(skipped, 0);
    }

    // ── filter_identical_files: LeftOnly/RightOnly キャッシュ比較テスト ──

    #[test]
    fn test_filter_identical_files_leftonly_cache_same_skipped() {
        // LeftOnly だがキャッシュ両方同一 → スキップされる
        let files = vec![("a.rs".to_string(), Badge::LeftOnly)];
        let local = make_cache(vec![("a.rs", "same")]);
        let remote = make_cache(vec![("a.rs", "same")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert!(filtered.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_filter_identical_files_leftonly_cache_one_side_passes() {
        // LeftOnly でキャッシュ片方のみ → 通常通り通過（安全側）
        let files = vec![("a.rs".to_string(), Badge::LeftOnly)];
        let local = make_cache(vec![("a.rs", "content")]);
        let remote = BoundedCache::new(100);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_identical_files_rightonly_cache_same_skipped() {
        // RightOnly だがキャッシュ両方同一 → スキップされる
        let files = vec![("a.rs".to_string(), Badge::RightOnly)];
        let local = make_cache(vec![("a.rs", "same")]);
        let remote = make_cache(vec![("a.rs", "same")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert!(filtered.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_filter_identical_files_leftonly_cache_different_passes() {
        // LeftOnly でキャッシュ両方異なる → 通過（マージ実行対象）
        let files = vec![("a.rs".to_string(), Badge::LeftOnly)];
        let local = make_cache(vec![("a.rs", "old")]);
        let remote = make_cache(vec![("a.rs", "new")]);

        let (filtered, skipped) = filter_identical_files(&files, &local, &remote);
        assert_eq!(filtered.len(), 1);
        assert_eq!(skipped, 0);
    }
}
