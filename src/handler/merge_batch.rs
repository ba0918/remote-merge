//! バッチマージ実行ロジック。
//!
//! ディレクトリ選択時のバッチマージと、
//! Unchecked ファイルの同一内容フィルタリング。

use crate::app::AppState;
use crate::merge::executor::MergeDirection;
use crate::runtime::TuiRuntime;
use crate::ui::dialog::{BatchConfirmDialog, DialogState, ProgressDialog, ProgressPhase};

use super::merge_file_io::{
    backup_left, backup_right, read_left_file, write_left_file, write_right_file,
};

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
                if state.is_connected {
                    backup_right(state, runtime, &file_paths);
                }
            }
            MergeDirection::RightToLeft => {
                backup_left(state, runtime, &file_paths);
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

                if !state.is_connected {
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

                match write_left_file(state, runtime, path, &content) {
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
        }
    }

    // プログレスダイアログを閉じる
    state.dialog = DialogState::None;

    // バッジ再計算（バッチ全体で1回だけ）
    if state.selected_path.is_some() {
        state.select_file();
    }
    state.rebuild_flat_nodes();

    let left = state.left_source.display_name();
    let right = state.right_source.display_name();
    let dir_str = match direction {
        MergeDirection::LeftToRight => format!("{} -> {}", left, right),
        MergeDirection::RightToLeft => format!("{} -> {}", right, left),
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
