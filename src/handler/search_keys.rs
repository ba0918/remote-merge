//! ファイル検索モード中のキー処理。

use crossterm::event::KeyCode;

use crate::app::search::{find_matches, nearest_match_from, next_match, prev_match};
use crate::app::AppState;
use crate::runtime::TuiRuntime;

/// 検索モード中のキーハンドリング。
///
/// 文字入力でクエリを更新し、インクリメンタルに最初の一致にジャンプする。
pub fn handle_search_key(state: &mut AppState, runtime: &mut TuiRuntime, code: KeyCode) {
    match code {
        KeyCode::Char(c) => {
            state.search_state.query.push(c);
            state.rebuild_flat_nodes();
            jump_to_first_match(state, runtime);
        }
        KeyCode::Backspace => {
            state.search_state.query.pop();
            state.rebuild_flat_nodes();
            if state.search_state.query.is_empty() {
                state.search_state.match_cursor = 0;
                update_search_status(state, 0);
            } else {
                jump_to_first_match(state, runtime);
            }
        }
        KeyCode::Enter => {
            // 検索モード終了（クエリ・結果は保持 → n/N で継続ジャンプ可能）
            state.search_state.deactivate();
        }
        KeyCode::Esc => {
            // 検索モード終了 + 結果クリア
            state.search_state.clear();
            state.rebuild_flat_nodes();
            state.status_message = String::new();
        }
        _ => {}
    }
}

/// 検索結果の次の一致にジャンプする（検索モード外でも動作）。
pub fn jump_next(state: &mut AppState, runtime: &mut TuiRuntime) {
    if !state.search_state.has_query() {
        return;
    }
    let matches = find_matches(&state.flat_nodes, &state.search_state.query);
    if matches.is_empty() {
        update_search_status(state, 0);
        return;
    }
    state.search_state.match_cursor = next_match(state.search_state.match_cursor, matches.len());
    jump_to_match(state, runtime, &matches);
}

/// 検索結果の前の一致にジャンプする（検索モード外でも動作）。
pub fn jump_prev(state: &mut AppState, runtime: &mut TuiRuntime) {
    if !state.search_state.has_query() {
        return;
    }
    let matches = find_matches(&state.flat_nodes, &state.search_state.query);
    if matches.is_empty() {
        update_search_status(state, 0);
        return;
    }
    state.search_state.match_cursor = prev_match(state.search_state.match_cursor, matches.len());
    jump_to_match(state, runtime, &matches);
}

/// 現在のカーソル位置から最も近い一致にジャンプする（インクリメンタルサーチ用）。
fn jump_to_first_match(state: &mut AppState, runtime: &mut TuiRuntime) {
    let matches = find_matches(&state.flat_nodes, &state.search_state.query);
    if matches.is_empty() {
        update_search_status(state, 0);
        return;
    }
    if let Some(match_idx) = nearest_match_from(&matches, state.tree_cursor) {
        state.search_state.match_cursor = match_idx;
        jump_to_match(state, runtime, &matches);
    }
}

/// match_cursor 位置の一致にカーソルを移動する。
///
/// コンテンツのロードは行わない（Enter/Right キーで明示的に選択する）。
/// これは vim の `/` 検索と同じ動作: カーソル移動のみ。
fn jump_to_match(state: &mut AppState, _runtime: &mut TuiRuntime, matches: &[usize]) {
    let total = matches.len();
    let cursor_in_matches = state.search_state.match_cursor;
    if let Some(&flat_idx) = matches.get(cursor_in_matches) {
        state.tree_cursor = flat_idx;
        state.ensure_cursor_visible();
        update_search_status_with_pos(state, cursor_in_matches + 1, total);
    }
}

/// 検索ステータスメッセージを更新する（一致なし）。
fn update_search_status(state: &mut AppState, total: usize) {
    if total == 0 && state.search_state.has_query() {
        state.status_message = format!("/{} [no match]", state.search_state.query);
    } else {
        state.status_message = format!("/{}", state.search_state.query);
    }
}

/// 検索ステータスメッセージを更新する（位置付き）。
fn update_search_status_with_pos(state: &mut AppState, pos: usize, total: usize) {
    state.status_message = format!("/{} [{}/{}]", state.search_state.query, pos, total);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    /// ルート直下にファイルノードを持つ AppState を作成する。
    /// rebuild_flat_nodes() で正しく flat_nodes が生成される。
    fn make_state_with_nodes(names: &[&str]) -> AppState {
        let nodes: Vec<FileNode> = names.iter().map(|n| FileNode::new_file(*n)).collect();
        let local_tree = FileTree {
            root: PathBuf::from("/test"),
            nodes,
        };
        let remote_tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![],
        };
        let mut state = AppState::new(local_tree, remote_tree, "test".to_string(), "default");
        state.rebuild_flat_nodes();
        state
    }

    #[test]
    fn test_char_input_updates_query() {
        let mut state = make_state_with_nodes(&["main.rs", "lib.rs"]);
        let mut runtime = make_runtime();
        state.search_state.activate();

        handle_search_key(&mut state, &mut runtime, KeyCode::Char('m'));
        assert_eq!(state.search_state.query, "m");

        handle_search_key(&mut state, &mut runtime, KeyCode::Char('a'));
        assert_eq!(state.search_state.query, "ma");
    }

    #[test]
    fn test_backspace_removes_char() {
        let mut state = make_state_with_nodes(&["main.rs"]);
        let mut runtime = make_runtime();
        state.search_state.activate();
        state.search_state.query = "mai".to_string();
        state.rebuild_flat_nodes();

        handle_search_key(&mut state, &mut runtime, KeyCode::Backspace);
        assert_eq!(state.search_state.query, "ma");
    }

    #[test]
    fn test_enter_deactivates_keeps_query() {
        let mut state = make_state_with_nodes(&["main.rs"]);
        let mut runtime = make_runtime();
        state.search_state.activate();
        state.search_state.query = "main".to_string();

        handle_search_key(&mut state, &mut runtime, KeyCode::Enter);
        assert!(!state.search_state.active);
        assert_eq!(state.search_state.query, "main");
    }

    #[test]
    fn test_esc_clears_search() {
        let mut state = make_state_with_nodes(&["main.rs"]);
        let mut runtime = make_runtime();
        state.search_state.activate();
        state.search_state.query = "main".to_string();

        handle_search_key(&mut state, &mut runtime, KeyCode::Esc);
        assert!(!state.search_state.active);
        assert!(state.search_state.query.is_empty());
    }

    #[test]
    fn test_jump_to_match_on_input() {
        let mut state = make_state_with_nodes(&["aaa.rs", "bbb.rs", "abc.rs"]);
        let mut runtime = make_runtime();
        state.search_state.activate();

        // "b" で bbb.rs にジャンプ — フィルタ後は bbb.rs が先頭
        handle_search_key(&mut state, &mut runtime, KeyCode::Char('b'));
        // フィルタリング後: [abc.rs, bbb.rs] — "b" にマッチするのは abc.rs(idx 0) と bbb.rs(idx 1)
        assert!(state.flat_nodes.iter().any(|n| n.name == "bbb.rs"));
        assert!(state.flat_nodes[state.tree_cursor].name.contains('b'));
    }

    #[test]
    fn test_jump_next_cycles() {
        // jump_next/jump_prev はフィルタリングなしの flat_nodes で動作
        // (検索モード外で使う n/N キー)
        let mut state = make_state_with_nodes(&["a.rs", "ab.rs", "b.rs", "abc.rs"]);
        let mut runtime = make_runtime();
        state.search_state.query = "a".to_string();
        state.search_state.match_cursor = 0;
        // rebuild でフィルタ適用: a.rs, ab.rs, abc.rs のみ表示
        state.rebuild_flat_nodes();

        // フィルタ後の全ノードが "a" にマッチ: [0, 1, 2]
        jump_next(&mut state, &mut runtime);
        assert_eq!(state.search_state.match_cursor, 1);

        jump_next(&mut state, &mut runtime);
        assert_eq!(state.search_state.match_cursor, 2);

        jump_next(&mut state, &mut runtime);
        assert_eq!(state.search_state.match_cursor, 0); // 循環
    }

    #[test]
    fn test_jump_prev_cycles() {
        let mut state = make_state_with_nodes(&["a.rs", "ab.rs", "b.rs", "abc.rs"]);
        let mut runtime = make_runtime();
        state.search_state.query = "a".to_string();
        state.search_state.match_cursor = 0;
        state.rebuild_flat_nodes();

        // フィルタ後: [a.rs, ab.rs, abc.rs] — 全マッチ [0, 1, 2]
        jump_prev(&mut state, &mut runtime);
        assert_eq!(state.search_state.match_cursor, 2); // 循環
    }

    #[test]
    fn test_search_filter_shows_only_matches() {
        let mut state = make_state_with_nodes(&["main.rs", "lib.rs", "mod.rs"]);
        state.search_state.activate();
        state.search_state.query = "main".to_string();
        state.rebuild_flat_nodes();

        assert_eq!(state.flat_nodes.len(), 1);
        assert_eq!(state.flat_nodes[0].name, "main.rs");
    }

    #[test]
    fn test_search_filter_clears_on_esc() {
        let mut state = make_state_with_nodes(&["main.rs", "lib.rs", "mod.rs"]);
        let mut runtime = make_runtime();
        state.search_state.activate();
        state.search_state.query = "main".to_string();
        state.rebuild_flat_nodes();
        assert_eq!(state.flat_nodes.len(), 1);

        handle_search_key(&mut state, &mut runtime, KeyCode::Esc);
        // Esc 後は全ファイルが表示される
        assert_eq!(state.flat_nodes.len(), 3);
    }

    fn make_runtime() -> TuiRuntime {
        TuiRuntime::new_for_test()
    }
}
