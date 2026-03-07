//! DiffView フォーカス時のキー処理。

use crossterm::event::KeyCode;

use crate::app::AppState;
use crate::diff::engine::HunkDirection;
use crate::runtime::TuiRuntime;
use crate::ui::dialog::DialogState;

use super::reconnect::execute_reconnect;

/// DiffView フォーカス時のキーハンドリング
pub fn handle_diff_key(state: &mut AppState, runtime: &mut TuiRuntime, code: KeyCode) {
    // Diff 検索モード中は diff_search_keys にディスパッチ
    if state.diff_search_state.active {
        super::diff_search_keys::handle_diff_search_key(state, code);
        return;
    }

    match code {
        KeyCode::Char('q') => {
            if state.has_unsaved_changes() {
                state.dialog = DialogState::UnsavedChanges;
            } else {
                state.should_quit = true;
            }
        }
        KeyCode::Esc => {
            if state.pending_hunk_merge.is_some() {
                state.cancel_hunk_merge();
            } else if state.diff_search_state.has_query() {
                state.diff_search_state.clear();
                state.status_message = String::new();
            } else if state.has_unsaved_changes() {
                state.dialog = DialogState::UnsavedChanges;
            } else {
                state.should_quit = true;
            }
        }
        KeyCode::Tab => {
            state.cancel_hunk_merge();
            state.toggle_focus();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.scroll_up();
            state.sync_hunk_cursor_to_scroll();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.scroll_down();
            state.sync_hunk_cursor_to_scroll();
        }
        KeyCode::Char('n') => {
            if state.diff_search_state.has_query() {
                super::diff_search_keys::jump_next_diff(state);
            } else {
                state.cancel_hunk_merge();
                state.hunk_cursor_down();
            }
        }
        KeyCode::Char('N') => {
            if state.diff_search_state.has_query() {
                super::diff_search_keys::jump_prev_diff(state);
            } else {
                state.cancel_hunk_merge();
                state.hunk_cursor_up();
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if state.is_connected {
                if state.hunk_count() > 0 {
                    state.apply_hunk_merge(HunkDirection::LeftToRight);
                }
            } else if state.hunk_count() > 0 {
                state.status_message =
                    "SSH not connected: cannot merge hunks to remote".to_string();
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if state.hunk_count() > 0 {
                state.apply_hunk_merge(HunkDirection::RightToLeft);
            }
        }
        KeyCode::Char('w') => {
            if state.has_unsaved_changes() {
                state.dialog = DialogState::WriteConfirmation;
            } else {
                state.status_message = "No changes to write".to_string();
            }
        }
        KeyCode::Char('u') => {
            state.undo_last();
        }
        KeyCode::Char('U') => {
            state.undo_all();
        }
        KeyCode::PageDown => {
            state.scroll_page_down(20);
            state.sync_hunk_cursor_to_scroll();
        }
        KeyCode::PageUp => {
            state.scroll_page_up(20);
            state.sync_hunk_cursor_to_scroll();
        }
        KeyCode::Home => {
            state.scroll_to_home();
            state.sync_hunk_cursor_to_scroll();
        }
        KeyCode::End => {
            state.scroll_to_end();
            state.sync_hunk_cursor_to_scroll();
        }
        KeyCode::Char('c') => super::tree_keys::handle_clipboard_copy(state),
        KeyCode::Char('r') => execute_reconnect(state, runtime),
        KeyCode::Char('d') => state.toggle_diff_mode(),
        KeyCode::Char('T') => state.cycle_theme(),
        KeyCode::Char('S') => state.toggle_syntax_highlight(),
        KeyCode::Char('/') => {
            state.diff_search_state.activate();
            state.status_message = "/".to_string();
        }
        KeyCode::Char('?') => state.show_help(),
        _ => {}
    }
}
