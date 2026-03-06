//! Diff View 検索モード中のキー処理。

use crossterm::event::KeyCode;

use crate::app::diff_search::{find_content_matches, find_diff_matches};
use crate::app::search::{
    format_search_status, format_search_status_with_pos, nearest_match_from, next_match, prev_match,
};
use crate::app::AppState;
use crate::diff::engine::DiffResult;

/// Diff 検索モード中のキーハンドリング。
pub fn handle_diff_search_key(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Char(c) => {
            state.diff_search_state.query.push(c);
            jump_to_first_diff_match(state);
        }
        KeyCode::Backspace => {
            state.diff_search_state.query.pop();
            if state.diff_search_state.query.is_empty() {
                state.diff_search_state.match_cursor = 0;
                update_diff_search_status(state, 0);
            } else {
                jump_to_first_diff_match(state);
            }
        }
        KeyCode::Enter => {
            state.diff_search_state.deactivate();
        }
        KeyCode::Esc => {
            state.diff_search_state.clear();
            state.status_message = String::new();
        }
        _ => {}
    }
}

/// 検索結果の次の一致にジャンプする。
pub fn jump_next_diff(state: &mut AppState) {
    if !state.diff_search_state.has_query() {
        return;
    }
    let matches = get_diff_matches(state);
    if matches.is_empty() {
        update_diff_search_status(state, 0);
        return;
    }
    state.diff_search_state.match_cursor =
        next_match(state.diff_search_state.match_cursor, matches.len());
    jump_to_diff_match(state, &matches);
}

/// 検索結果の前の一致にジャンプする。
pub fn jump_prev_diff(state: &mut AppState) {
    if !state.diff_search_state.has_query() {
        return;
    }
    let matches = get_diff_matches(state);
    if matches.is_empty() {
        update_diff_search_status(state, 0);
        return;
    }
    state.diff_search_state.match_cursor =
        prev_match(state.diff_search_state.match_cursor, matches.len());
    jump_to_diff_match(state, &matches);
}

/// 現在のカーソル位置から最も近いマッチにジャンプする。
fn jump_to_first_diff_match(state: &mut AppState) {
    let matches = get_diff_matches(state);
    if matches.is_empty() {
        update_diff_search_status(state, 0);
        return;
    }
    if let Some(match_idx) = nearest_match_from(&matches, state.diff_cursor) {
        state.diff_search_state.match_cursor = match_idx;
        jump_to_diff_match(state, &matches);
    }
}

/// match_cursor 位置の行にカーソルを移動する。
fn jump_to_diff_match(state: &mut AppState, matches: &[usize]) {
    let total = matches.len();
    let cursor_in_matches = state.diff_search_state.match_cursor;
    if let Some(&line_idx) = matches.get(cursor_in_matches) {
        state.diff_cursor = line_idx;
        state.ensure_cursor_visible();
        update_diff_search_status_with_pos(state, cursor_in_matches + 1, total);
    }
}

/// 現在の diff から検索マッチを取得する。
fn get_diff_matches(state: &AppState) -> Vec<usize> {
    let query = &state.diff_search_state.query;
    match &state.current_diff {
        Some(DiffResult::Modified { lines, .. }) => find_diff_matches(lines, query),
        Some(DiffResult::Equal) => state
            .selected_path
            .as_ref()
            .and_then(|p| state.left_cache.get(p))
            .map(|content| find_content_matches(content, query))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// ステータスメッセージ更新（一致なし）
fn update_diff_search_status(state: &mut AppState, total: usize) {
    state.status_message = format_search_status(&state.diff_search_state.query, total);
}

/// ステータスメッセージ更新（位置付き）
fn update_diff_search_status_with_pos(state: &mut AppState, pos: usize, total: usize) {
    state.status_message =
        format_search_status_with_pos(&state.diff_search_state.query, pos, total);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Side;
    use crate::diff::engine::{DiffLine, DiffResult, DiffStats, DiffTag};
    use crate::tree::FileTree;
    use std::path::PathBuf;

    fn make_state_with_diff(lines: Vec<DiffLine>) -> AppState {
        let mut state = AppState::new(
            FileTree {
                root: PathBuf::from("/test"),
                nodes: vec![],
            },
            FileTree {
                root: PathBuf::from("/test"),
                nodes: vec![],
            },
            Side::Local,
            Side::Remote("test".to_string()),
            "default",
        );
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            merge_hunks: vec![],
            lines,
            stats: DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            merge_hunk_line_indices: vec![],
        });
        state
    }

    fn make_lines(values: &[&str]) -> Vec<DiffLine> {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| DiffLine {
                tag: DiffTag::Equal,
                value: v.to_string(),
                old_index: Some(i),
                new_index: Some(i),
            })
            .collect()
    }

    #[test]
    fn test_char_input_updates_query() {
        let mut state = make_state_with_diff(make_lines(&["hello", "world"]));
        state.diff_search_state.activate();

        handle_diff_search_key(&mut state, KeyCode::Char('h'));
        assert_eq!(state.diff_search_state.query, "h");
        assert_eq!(state.diff_cursor, 0); // "hello" にジャンプ
    }

    #[test]
    fn test_enter_deactivates() {
        let mut state = make_state_with_diff(make_lines(&["hello"]));
        state.diff_search_state.activate();
        state.diff_search_state.query = "hello".to_string();

        handle_diff_search_key(&mut state, KeyCode::Enter);
        assert!(!state.diff_search_state.active);
        assert_eq!(state.diff_search_state.query, "hello");
    }

    #[test]
    fn test_esc_clears() {
        let mut state = make_state_with_diff(make_lines(&["hello"]));
        state.diff_search_state.activate();
        state.diff_search_state.query = "hello".to_string();

        handle_diff_search_key(&mut state, KeyCode::Esc);
        assert!(!state.diff_search_state.active);
        assert!(state.diff_search_state.query.is_empty());
    }

    #[test]
    fn test_jump_next_cycles() {
        let mut state = make_state_with_diff(make_lines(&["aaa", "bbb", "aaa", "ccc", "aaa"]));
        state.diff_search_state.query = "aaa".to_string();
        state.diff_search_state.match_cursor = 0;

        jump_next_diff(&mut state);
        assert_eq!(state.diff_cursor, 2);

        jump_next_diff(&mut state);
        assert_eq!(state.diff_cursor, 4);

        jump_next_diff(&mut state);
        assert_eq!(state.diff_cursor, 0); // 循環
    }

    #[test]
    fn test_jump_prev_cycles() {
        let mut state = make_state_with_diff(make_lines(&["aaa", "bbb", "aaa"]));
        state.diff_search_state.query = "aaa".to_string();
        state.diff_search_state.match_cursor = 0;

        jump_prev_diff(&mut state);
        assert_eq!(state.diff_cursor, 2); // 循環
    }

    #[test]
    fn test_no_match_status() {
        let mut state = make_state_with_diff(make_lines(&["hello"]));
        state.diff_search_state.activate();
        handle_diff_search_key(&mut state, KeyCode::Char('z'));
        assert!(state.status_message.contains("no match"));
    }
}
