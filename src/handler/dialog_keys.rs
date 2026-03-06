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
                    if server_name != state.server_name {
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
                        let (source_name, target_name) = match direction {
                            crate::merge::executor::MergeDirection::LeftToRight => {
                                ("local".to_string(), state.server_name.clone())
                            }
                            crate::merge::executor::MergeDirection::RightToLeft => {
                                (state.server_name.clone(), "local".to_string())
                            }
                        };
                        let confirm = crate::ui::dialog::ConfirmDialog {
                            file_path: path.clone(),
                            direction,
                            source_name,
                            target_name,
                            is_remote_to_remote: false,
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
        DialogState::None => {}
    }
}
