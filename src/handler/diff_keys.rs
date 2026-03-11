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
            if state.showing_ref_diff {
                state.status_message = "Read-only: ref diff. Press X to swap and merge".to_string();
            } else if runtime.is_side_available(&state.right_source) {
                if state.hunk_count() > 0 {
                    state.apply_hunk_merge(HunkDirection::LeftToRight);
                }
            } else if state.hunk_count() > 0 {
                state.status_message = "Right side not available: cannot merge hunks".to_string();
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if state.showing_ref_diff {
                state.status_message = "Read-only: ref diff. Press X to swap and merge".to_string();
            } else if state.hunk_count() > 0 {
                state.apply_hunk_merge(HunkDirection::RightToLeft);
            }
        }
        KeyCode::Char('w') => {
            if state.showing_ref_diff {
                state.status_message = "Read-only: ref diff. Press X to swap and merge".to_string();
            } else if state.has_unsaved_changes() {
                state.dialog = DialogState::WriteConfirmation;
            } else {
                state.status_message = "No changes to write".to_string();
            }
        }
        KeyCode::Char('u') => {
            if state.showing_ref_diff {
                state.status_message = "Read-only: ref diff. Press X to swap and merge".to_string();
            } else {
                state.undo_last();
            }
        }
        KeyCode::Char('U') => {
            if state.showing_ref_diff {
                state.status_message = "Read-only: ref diff. Press X to swap and merge".to_string();
            } else {
                state.undo_all();
            }
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
        KeyCode::Char('W') => {
            super::three_way_summary_handler::open_three_way_summary(state);
        }
        KeyCode::Char('X') => {
            if state.has_reference() {
                super::reconnect::execute_ref_swap(state, runtime);
            } else {
                state.status_message = "No reference server".to_string();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{AppState, Side};
    use crate::diff::engine::{DiffLine, DiffResult, DiffStats, DiffTag, HunkDirection};
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_test_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_test_state() -> AppState {
        AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        )
    }

    fn make_diff_lines(n: usize) -> Vec<DiffLine> {
        (0..n)
            .map(|i| DiffLine {
                tag: DiffTag::Equal,
                value: format!("line {}", i),
                old_index: Some(i),
                new_index: Some(i),
            })
            .collect()
    }

    fn make_state_with_diff_lines(n: usize) -> AppState {
        let mut state = make_test_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: DiffStats {
                insertions: 0,
                deletions: 0,
                equal: n,
            },
            lines: make_diff_lines(n),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        state
    }

    // 1. scroll_up / scroll_down で diff_cursor が変化する
    #[test]
    fn test_scroll_up_down() {
        let mut state = make_state_with_diff_lines(30);
        assert_eq!(state.diff_cursor, 0);

        state.scroll_down();
        assert_eq!(state.diff_cursor, 1);

        state.scroll_down();
        assert_eq!(state.diff_cursor, 2);

        state.scroll_up();
        assert_eq!(state.diff_cursor, 1);

        // 0 で scroll_up しても 0 のまま
        state.scroll_up();
        state.scroll_up();
        assert_eq!(state.diff_cursor, 0);
    }

    // 2. scroll_to_home → 0, scroll_to_end → max
    #[test]
    fn test_scroll_to_home_end() {
        let mut state = make_state_with_diff_lines(50);

        state.scroll_to_end();
        assert_eq!(state.diff_cursor, 49);

        state.scroll_to_home();
        assert_eq!(state.diff_cursor, 0);
        assert_eq!(state.diff_scroll, 0);
    }

    // NOTE: toggle_focus, toggle_diff_mode, show_help は
    // handler/tree_keys.rs のテストでカバー済み

    // 3. diff_search_state.activate() で active=true
    #[test]
    fn test_search_activate() {
        let mut state = make_test_state();
        assert!(!state.diff_search_state.active);

        state.diff_search_state.activate();
        assert!(state.diff_search_state.active);
        assert!(state.diff_search_state.query.is_empty());
    }

    // 7. diff_search_state.clear() で active=false, query が空
    #[test]
    fn test_search_clear() {
        let mut state = make_test_state();
        state.diff_search_state.activate();
        state.diff_search_state.query = "test query".to_string();
        assert!(state.diff_search_state.active);
        assert!(!state.diff_search_state.query.is_empty());

        state.diff_search_state.clear();
        assert!(!state.diff_search_state.active);
        assert!(state.diff_search_state.query.is_empty());
        assert_eq!(state.diff_search_state.match_cursor, 0);
    }

    // 8. cancel_hunk_merge() で pending_hunk_merge=None
    #[test]
    fn test_cancel_hunk_merge() {
        let mut state = make_test_state();
        state.pending_hunk_merge = Some(HunkDirection::LeftToRight);

        state.cancel_hunk_merge();
        assert!(state.pending_hunk_merge.is_none());

        // 既に None の場合は何も変わらない
        let msg_before = state.status_message.clone();
        state.cancel_hunk_merge();
        assert_eq!(state.status_message, msg_before);
    }

    // 9. has_unsaved_changes() の初期値は false
    #[test]
    fn test_unsaved_changes_check() {
        let state = make_test_state();
        assert!(!state.has_unsaved_changes());
        assert!(state.undo_stack.is_empty());
    }

    // 10. scroll_page_down / scroll_page_up
    #[test]
    fn test_page_scroll() {
        let mut state = make_state_with_diff_lines(100);
        assert_eq!(state.diff_cursor, 0);

        state.scroll_page_down(20);
        assert_eq!(state.diff_cursor, 20);

        state.scroll_page_down(20);
        assert_eq!(state.diff_cursor, 40);

        state.scroll_page_up(20);
        assert_eq!(state.diff_cursor, 20);

        state.scroll_page_up(20);
        assert_eq!(state.diff_cursor, 0);

        // 0 未満にはならない
        state.scroll_page_up(20);
        assert_eq!(state.diff_cursor, 0);

        // 末尾を超えない
        state.scroll_page_down(200);
        assert_eq!(state.diff_cursor, 99);
    }
}
