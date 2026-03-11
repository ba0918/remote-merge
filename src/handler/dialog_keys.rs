//! ダイアログ表示中のキー処理。

use crossterm::event::KeyCode;

use crate::app::AppState;
use crate::runtime::TuiRuntime;
use crate::ui::dialog::DialogState;

use super::merge_exec::{
    check_mtime_conflict_single, check_mtime_for_write, execute_batch_merge, execute_hunk_merge,
    execute_merge, execute_write_changes,
};
use super::reconnect::execute_server_switch;

/// ダイアログ表示中のキーハンドリング
pub fn handle_dialog_key(state: &mut AppState, runtime: &mut TuiRuntime, key: KeyCode) {
    match &state.dialog {
        DialogState::Confirm(_) => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let confirm = match &state.dialog {
                    DialogState::Confirm(c) => c.clone(),
                    _ => unreachable!(),
                };
                state.close_dialog();
                // 楽観的ロック: mtime チェック
                if check_mtime_conflict_single(
                    state,
                    runtime,
                    &confirm.file_path,
                    confirm.direction,
                ) {
                    return; // MtimeWarningDialog が表示される
                }
                execute_merge(state, runtime, &confirm);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.status_message = "Merge cancelled".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::BatchConfirm(_) => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let batch = match &state.dialog {
                    DialogState::BatchConfirm(b) => b.clone(),
                    _ => unreachable!(),
                };
                state.close_dialog();
                execute_batch_merge(state, runtime, &batch);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.status_message = "Batch merge cancelled".to_string();
                state.close_dialog();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::BatchConfirm(ref mut b) = state.dialog {
                    b.scroll_down();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::BatchConfirm(ref mut b) = state.dialog {
                    b.scroll_up();
                }
            }
            _ => {}
        },
        DialogState::ServerSelect(_) => match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::ServerSelect(ref mut m) = state.dialog {
                    m.cursor_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::ServerSelect(ref mut m) = state.dialog {
                    m.cursor_down();
                }
            }
            KeyCode::Enter => {
                let selected = if let DialogState::ServerSelect(ref menu) = state.dialog {
                    menu.selected().map(|s| s.to_string())
                } else {
                    None
                };
                if let Some(server_name) = selected {
                    if server_name != state.right_source.display_name() {
                        execute_server_switch(state, runtime, &server_name);
                    }
                }
                state.close_dialog();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::Filter(_) => match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::Filter(ref mut panel) = state.dialog {
                    panel.cursor_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::Filter(ref mut panel) = state.dialog {
                    panel.cursor_down();
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let DialogState::Filter(ref mut panel) = state.dialog {
                    panel.toggle();
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                let panel_clone = if let DialogState::Filter(ref panel) = state.dialog {
                    Some(panel.clone())
                } else {
                    None
                };
                if let Some(ref panel) = panel_clone {
                    state.apply_filter_changes(panel);
                }
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::Help(_) => match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                state.close_dialog();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::Help(ref mut help) = state.dialog {
                    help.scroll_down();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::Help(ref mut help) = state.dialog {
                    help.scroll_up();
                }
            }
            KeyCode::PageDown => {
                if let DialogState::Help(ref mut help) = state.dialog {
                    help.page_down(10);
                }
            }
            KeyCode::PageUp => {
                if let DialogState::Help(ref mut help) = state.dialog {
                    help.page_up(10);
                }
            }
            _ => {}
        },
        DialogState::HunkMergePreview(ref preview) => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let direction = preview.direction;
                state.pending_hunk_merge = None;
                state.close_dialog();
                // 楽観的ロック: mtime チェック（書き込み先）
                if check_mtime_for_write(state, runtime, Some(direction)) {
                    return; // MtimeWarningDialog が表示される
                }
                execute_hunk_merge(state, runtime, direction);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.pending_hunk_merge = None;
                state.status_message = "Hunk merge cancelled".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::WriteConfirmation => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                state.close_dialog();
                // 楽観的ロック: mtime チェック（両側書き込み）
                if check_mtime_for_write(state, runtime, None) {
                    return; // MtimeWarningDialog が表示される
                }
                execute_write_changes(state, runtime);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.status_message = "Write cancelled".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::UnsavedChanges => match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                state.undo_stack.clear();
                state.close_dialog();
                state.should_quit = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.status_message = "Quit cancelled | w:write u:undo".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::Info(_) => match key {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::Progress(ref progress) => match key {
            KeyCode::Esc if progress.cancelable => {
                // マージスキャンのキャンセルは tree_keys.rs 側で処理
                // ここでは走査中でなければダイアログを閉じる
                if matches!(state.merge_scan_state, crate::app::MergeScanState::Idle) {
                    state.close_dialog();
                }
                // 走査中の場合は tree_keys.rs の Esc ハンドラがキャンセルする
            }
            _ => {}
        },
        DialogState::MtimeWarning(_) => match key {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // reload: ファイル再読み込み
                state.status_message = "Reloading file...".to_string();
                state.close_dialog();
                if let Some(path) = state.selected_path.clone() {
                    // キャッシュクリアして再読み込みをトリガー
                    state.invalidate_cache_for_paths(&[path]);
                    super::merge_exec::load_file_content(state, runtime);
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // force: 強制マージ続行
                let dialog = match &state.dialog {
                    DialogState::MtimeWarning(d) => d.clone(),
                    _ => unreachable!(),
                };
                state.close_dialog();
                match dialog.merge_context {
                    crate::ui::dialog::MtimeWarningMergeContext::Single {
                        ref path,
                        direction,
                    } => {
                        let left_name = state.left_source.display_name().to_string();
                        let right_name = state.right_source.display_name().to_string();
                        let (source_name, target_name) = match direction {
                            crate::merge::executor::MergeDirection::LeftToRight => {
                                (left_name, right_name)
                            }
                            crate::merge::executor::MergeDirection::RightToLeft => {
                                (right_name, left_name)
                            }
                        };
                        let confirm = crate::ui::dialog::ConfirmDialog {
                            file_path: path.clone(),
                            direction,
                            source_name,
                            target_name,
                            is_remote_to_remote: state.is_remote_to_remote(),
                        };
                        execute_merge(state, runtime, &confirm);
                    }
                    crate::ui::dialog::MtimeWarningMergeContext::Batch { .. } => {
                        state.status_message =
                            "Force merge for batch not yet supported".to_string();
                    }
                    crate::ui::dialog::MtimeWarningMergeContext::Write => {
                        execute_write_changes(state, runtime);
                    }
                    crate::ui::dialog::MtimeWarningMergeContext::HunkMerge { direction } => {
                        execute_hunk_merge(state, runtime, direction);
                    }
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                state.status_message = "Merge cancelled (mtime conflict)".to_string();
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::ThreeWaySummary(_) => match key {
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::ThreeWaySummary(ref mut panel) = state.dialog {
                    panel.cursor_down();
                    panel.adjust_scroll(15);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::ThreeWaySummary(ref mut panel) = state.dialog {
                    panel.cursor_up();
                    panel.adjust_scroll(15);
                }
            }
            KeyCode::Enter => {
                let jump_idx = if let DialogState::ThreeWaySummary(ref panel) = state.dialog {
                    panel.selected_diff_line_index()
                } else {
                    None
                };
                state.close_dialog();
                if let Some(idx) = jump_idx {
                    state.diff_cursor = idx;
                    // diff_visible_height は draw_ui で毎フレーム更新される。
                    // 初回描画前に 0 の場合は diff_scroll = idx となるが、
                    // ダイアログが開かれるのは描画後なので実用上問題ない。
                    let half = state.diff_visible_height / 2;
                    state.diff_scroll = idx.saturating_sub(half);
                }
            }
            KeyCode::Char('W') | KeyCode::Esc => {
                state.close_dialog();
            }
            KeyCode::PageDown => {
                if let DialogState::ThreeWaySummary(ref mut panel) = state.dialog {
                    panel.page_down(15);
                    panel.adjust_scroll(15);
                }
            }
            KeyCode::PageUp => {
                if let DialogState::ThreeWaySummary(ref mut panel) = state.dialog {
                    panel.page_up(15);
                    panel.adjust_scroll(15);
                }
            }
            _ => {}
        },
        DialogState::PairServerSelect(_) => match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let DialogState::PairServerSelect(ref mut m) = state.dialog {
                    m.cursor_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let DialogState::PairServerSelect(ref mut m) = state.dialog {
                    m.cursor_down();
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                if let DialogState::PairServerSelect(ref mut m) = state.dialog {
                    m.toggle_column();
                }
            }
            KeyCode::Enter => {
                let pair = if let DialogState::PairServerSelect(ref menu) = state.dialog {
                    if menu.is_same_pair() {
                        None // 同じサーバ同士は不可
                    } else {
                        Some((
                            menu.selected_left().map(|s| s.to_string()),
                            menu.selected_right().map(|s| s.to_string()),
                        ))
                    }
                } else {
                    None
                };
                if let Some((Some(left_name), Some(right_name))) = pair {
                    state.close_dialog();
                    super::reconnect::execute_pair_switch(state, runtime, &left_name, &right_name);
                }
                // is_same_pair() の場合は閉じない（ユーザーにエラーを認識させる）
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                state.close_dialog();
            }
            _ => {}
        },
        DialogState::None => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::app::three_way_summary::{SummaryLine, ThreeWaySummaryPanel};
    use crate::app::AppState;
    use crate::app::Side;
    use crate::tree::FileTree;
    use crate::ui::dialog::{BatchConfirmDialog, DialogState, FilterPanel, HelpOverlay};

    fn make_test_state() -> AppState {
        AppState::new(
            FileTree::default(),
            FileTree::default(),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        )
    }

    // -------------------------------------------------------
    // Help dialog
    // -------------------------------------------------------

    #[test]
    fn test_help_dialog_close() {
        let mut state = make_test_state();
        state.dialog = DialogState::Help(HelpOverlay::new());
        assert!(matches!(state.dialog, DialogState::Help(_)));
        state.close_dialog();
        assert!(matches!(state.dialog, DialogState::None));
    }

    #[test]
    fn test_help_dialog_scroll_down_and_up() {
        let mut help = HelpOverlay::new();
        assert_eq!(help.scroll, 0);

        help.scroll_down();
        assert_eq!(help.scroll, 1);

        help.scroll_down();
        assert_eq!(help.scroll, 2);

        help.scroll_up();
        assert_eq!(help.scroll, 1);

        help.scroll_up();
        assert_eq!(help.scroll, 0);

        // 0 未満にはならない
        help.scroll_up();
        assert_eq!(help.scroll, 0);
    }

    // -------------------------------------------------------
    // Info dialog
    // -------------------------------------------------------

    #[test]
    fn test_info_dialog_close() {
        let mut state = make_test_state();
        state.dialog = DialogState::Info("test message".to_string());
        assert!(matches!(state.dialog, DialogState::Info(_)));
        state.close_dialog();
        assert!(matches!(state.dialog, DialogState::None));
    }

    // -------------------------------------------------------
    // UnsavedChanges dialog
    // -------------------------------------------------------

    // NOTE: handle_dialog_key() は TuiRuntime を要求するため直接呼べない。
    // 以下は UnsavedChanges ダイアログに関連する AppState メソッドの動作テスト。

    #[test]
    fn test_has_unsaved_changes_with_undo_stack() {
        let mut state = make_test_state();
        assert!(!state.has_unsaved_changes());

        state.undo_stack.push_back(crate::app::CacheSnapshot {
            local_content: String::new(),
            remote_content: String::new(),
            diff: None,
        });
        assert!(state.has_unsaved_changes());

        state.undo_stack.clear();
        assert!(!state.has_unsaved_changes());
    }

    #[test]
    fn test_unsaved_changes_dialog_opens_on_quit_attempt() {
        let mut state = make_test_state();
        // undo_stack に変更があるとき、quit 試行で UnsavedChanges ダイアログが開く
        state.undo_stack.push_back(crate::app::CacheSnapshot {
            local_content: String::new(),
            remote_content: String::new(),
            diff: None,
        });

        // handle_diff_key の q パス相当: has_unsaved_changes() → UnsavedChanges
        if state.has_unsaved_changes() {
            state.dialog = DialogState::UnsavedChanges;
        } else {
            state.should_quit = true;
        }

        assert!(!state.should_quit);
        assert!(matches!(state.dialog, DialogState::UnsavedChanges));
    }

    #[test]
    fn test_close_dialog_returns_to_none() {
        let mut state = make_test_state();
        state.dialog = DialogState::UnsavedChanges;
        state.close_dialog();
        assert!(matches!(state.dialog, DialogState::None));
    }

    // -------------------------------------------------------
    // FilterPanel
    // -------------------------------------------------------

    #[test]
    fn test_filter_panel_cursor_navigation() {
        let mut panel = FilterPanel::new(&[
            "node_modules".to_string(),
            ".git".to_string(),
            "dist".to_string(),
        ]);
        assert_eq!(panel.cursor, 0);

        panel.cursor_down();
        assert_eq!(panel.cursor, 1);

        panel.cursor_down();
        assert_eq!(panel.cursor, 2);

        // 最下端で止まる
        panel.cursor_down();
        assert_eq!(panel.cursor, 2);

        panel.cursor_up();
        assert_eq!(panel.cursor, 1);

        panel.cursor_up();
        assert_eq!(panel.cursor, 0);

        // 最上端で止まる
        panel.cursor_up();
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn test_filter_panel_toggle() {
        let mut panel = FilterPanel::new(&["*.log".to_string(), "*.tmp".to_string()]);
        assert!(panel.patterns[0].1); // 初期状態: 全て有効

        panel.toggle();
        assert!(!panel.patterns[0].1); // cursor=0 のパターンが無効に
        assert_eq!(panel.active_patterns(), vec!["*.tmp"]);

        panel.toggle();
        assert!(panel.patterns[0].1); // 再トグルで有効に戻る
        assert_eq!(panel.active_patterns().len(), 2);
    }

    // -------------------------------------------------------
    // ThreeWaySummary
    // -------------------------------------------------------

    fn make_summary_lines(n: usize) -> Vec<SummaryLine> {
        (0..n)
            .map(|i| SummaryLine {
                display_line_number: Some(i + 1),
                diff_line_index: i * 2,
                left_content: Some(format!("left_{}", i)),
                right_content: Some(format!("right_{}", i)),
                ref_content: Some(format!("ref_{}", i)),
            })
            .collect()
    }

    #[test]
    fn test_three_way_summary_cursor_movement() {
        let mut panel = ThreeWaySummaryPanel::new(
            "test.rs".to_string(),
            make_summary_lines(5),
            "local".to_string(),
            "develop".to_string(),
            "release".to_string(),
        );
        assert_eq!(panel.cursor, 0);

        panel.cursor_down();
        assert_eq!(panel.cursor, 1);

        panel.cursor_down();
        panel.cursor_down();
        panel.cursor_down();
        assert_eq!(panel.cursor, 4);

        // 最下端で止まる
        panel.cursor_down();
        assert_eq!(panel.cursor, 4);

        panel.cursor_up();
        assert_eq!(panel.cursor, 3);
    }

    #[test]
    fn test_three_way_summary_close() {
        let mut state = make_test_state();
        let panel = ThreeWaySummaryPanel::new(
            "test.rs".to_string(),
            make_summary_lines(3),
            "local".to_string(),
            "develop".to_string(),
            "release".to_string(),
        );
        state.dialog = DialogState::ThreeWaySummary(panel);
        assert!(matches!(state.dialog, DialogState::ThreeWaySummary(_)));

        state.close_dialog();
        assert!(matches!(state.dialog, DialogState::None));
    }

    #[test]
    fn test_three_way_summary_selected_diff_line_index() {
        let mut panel = ThreeWaySummaryPanel::new(
            "test.rs".to_string(),
            make_summary_lines(3),
            "local".to_string(),
            "develop".to_string(),
            "release".to_string(),
        );
        // cursor=0 → diff_line_index = 0*2 = 0
        assert_eq!(panel.selected_diff_line_index(), Some(0));

        panel.cursor_down();
        // cursor=1 → diff_line_index = 1*2 = 2
        assert_eq!(panel.selected_diff_line_index(), Some(2));
    }

    // -------------------------------------------------------
    // BatchConfirmDialog scroll
    // -------------------------------------------------------

    #[test]
    fn test_batch_confirm_scroll() {
        use crate::app::Badge;
        use crate::merge::executor::MergeDirection;

        let files: Vec<(String, Badge)> = (0..30)
            .map(|i| (format!("file{}.txt", i), Badge::Modified))
            .collect();
        let mut batch = BatchConfirmDialog::new(
            files,
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
            0,
        );
        assert_eq!(batch.scroll, 0);

        batch.scroll_down();
        assert_eq!(batch.scroll, 1);

        batch.scroll_down();
        assert_eq!(batch.scroll, 2);

        batch.scroll_up();
        assert_eq!(batch.scroll, 1);

        batch.scroll_up();
        assert_eq!(batch.scroll, 0);

        // 0 未満にはならない
        batch.scroll_up();
        assert_eq!(batch.scroll, 0);

        // 最大値で止まる
        for _ in 0..50 {
            batch.scroll_down();
        }
        assert_eq!(batch.scroll, 29); // files.len() - 1
    }
}
