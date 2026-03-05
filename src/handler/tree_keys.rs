//! FileTree フォーカス時のキー処理。

use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{AppState, ScanState};
use crate::merge::executor::MergeDirection;
use crate::runtime::scanner;
use crate::runtime::TuiRuntime;

use super::merge_exec::load_file_content;
use super::reconnect::execute_reconnect;

/// FileTree フォーカス時のキーハンドリング
pub fn handle_tree_key(
    state: &mut AppState,
    runtime: &mut TuiRuntime,
    code: KeyCode,
    _modifiers: KeyModifiers,
) {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            if matches!(state.scan_state, ScanState::Scanning) && code == KeyCode::Esc {
                state.scan_state = ScanState::Idle;
                runtime.scan_receiver = None;
                state.status_message = "Scan cancelled".to_string();
            } else {
                state.should_quit = true;
            }
        }
        KeyCode::Tab => state.toggle_focus(),
        KeyCode::Up | KeyCode::Char('k') => state.cursor_up(),
        KeyCode::Down | KeyCode::Char('j') => state.cursor_down(),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
            if state
                .flat_nodes
                .get(state.tree_cursor)
                .is_some_and(|n| n.is_dir)
            {
                if let Some(path) = state.current_path() {
                    let needs_load = state
                        .local_tree
                        .find_node(std::path::Path::new(&path))
                        .is_some_and(|n| n.is_dir() && !n.is_loaded());
                    if needs_load {
                        state.load_local_children(&path);
                        if state.is_connected {
                            super::merge_exec::load_remote_children(state, runtime, &path);
                        }
                    }
                }
                state.toggle_expand();
            } else {
                load_file_content(state, runtime);
                state.select_file();
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if state
                .flat_nodes
                .get(state.tree_cursor)
                .is_some_and(|n| n.is_dir && n.expanded)
            {
                state.toggle_expand();
            }
        }
        KeyCode::Char('r') => {
            if state.current_is_dir() {
                if let Some(path) = state.current_path() {
                    state.refresh_directory(&path);
                }
            } else {
                state.clear_cache();
            }
            state.clear_scan_cache();
        }
        KeyCode::Char('f') => state.show_filter_panel(),
        KeyCode::Char('s') => state.show_server_menu(),
        KeyCode::Char('c') => execute_reconnect(state, runtime),
        KeyCode::Char('?') => state.show_help(),
        KeyCode::Char('F') => scanner::handle_diff_filter_toggle(state, runtime),
        KeyCode::Char('L') => state.show_merge_dialog(MergeDirection::LocalToRemote),
        KeyCode::Char('R') => state.show_merge_dialog(MergeDirection::RemoteToLocal),
        _ => {}
    }
}
