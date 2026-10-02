//! Undo スタック管理。
//!
//! ハンクマージ操作の undo/redo を管理する。
//! CacheSnapshot のスタック操作と AppState への復元を行う。

use super::types::{CacheSnapshot, MAX_UNDO_STACK};
use super::AppState;
use crate::diff::engine;

impl AppState {
    /// undo 用スナップショットをスタックに保存する
    pub fn push_undo_snapshot(&mut self, snapshot: CacheSnapshot) {
        if self.undo_stack.len() >= MAX_UNDO_STACK {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(snapshot);
    }

    /// 最後のハンク操作を undo する
    pub fn undo_last(&mut self) -> bool {
        if let Some(snapshot) = self.undo_stack.pop_back() {
            if let Some(path) = self.selected_path.clone() {
                self.restore_snapshot(snapshot, &path);
                self.status_message = format!(
                    "Undo | {} changes remaining | w:write u:undo",
                    self.undo_stack.len(),
                );
                return true;
            }
        }
        self.status_message = "Nothing to undo".to_string();
        false
    }

    /// 全ハンク操作を undo する（初期状態に復元）
    ///
    /// `selected_path` が None の場合はスタックを消費しない（データ消失バグ防止）。
    pub fn undo_all(&mut self) -> bool {
        if self.undo_stack.is_empty() {
            self.status_message = "Nothing to undo".to_string();
            return false;
        }

        // selected_path を先にチェック: None の場合はスタックを消費しない
        let path = match self.selected_path.clone() {
            Some(p) => p,
            None => return false,
        };

        if let Some(initial) = self.undo_stack.pop_front() {
            self.undo_stack.clear();
            self.restore_snapshot(initial, &path);
            self.status_message = "All changes undone".to_string();
            return true;
        }
        false
    }

    /// 未保存の変更があるかどうか
    pub fn has_unsaved_changes(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// スナップショットからキャッシュ・diff・ハンクカーソルを復元する
    ///
    /// diff は保存していないため `compute_diff()` で再計算する。
    fn restore_snapshot(&mut self, snapshot: CacheSnapshot, path: &str) {
        // 先に diff を計算（参照のみで clone 不要）
        self.current_diff = Some(engine::compute_diff(
            &snapshot.local_content,
            &snapshot.remote_content,
        ));
        // move で insert（所有権移動により clone 不要）
        self.left_cache
            .insert(path.to_string(), snapshot.local_content);
        self.right_cache
            .insert(path.to_string(), snapshot.remote_content);

        self.clamp_hunk_cursor();

        // シンタックスハイライトキャッシュを両側再構築
        self.highlight_cache_left.remove(path);
        self.highlight_cache_right.remove(path);
        self.build_highlight_cache(path);

        self.rebuild_flat_nodes();
    }

    /// ハンクカーソルを有効範囲内に収める
    pub fn clamp_hunk_cursor(&mut self) {
        let new_count = self.hunk_count();
        if new_count == 0 {
            self.hunk_cursor = 0;
        } else if self.hunk_cursor >= new_count {
            self.hunk_cursor = new_count - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::types::CacheSnapshot;
    use crate::app::AppState;
    use crate::app::Side;
    use crate::tree::FileTree;

    fn make_state() -> AppState {
        AppState::new(
            FileTree::default(),
            FileTree::default(),
            Side::Local,
            Side::new("develop"),
            crate::theme::DEFAULT_THEME,
        )
    }

    fn make_snapshot(local: &str, remote: &str) -> CacheSnapshot {
        CacheSnapshot {
            local_content: local.to_string(),
            remote_content: remote.to_string(),
        }
    }

    // ── undo_all: selected_path = None のときスタックを保持する ──

    #[test]
    fn test_undo_all_no_selected_path_preserves_stack() {
        let mut state = make_state();
        state.selected_path = None;
        state.undo_stack.push_back(make_snapshot("v0", "r0"));
        state.undo_stack.push_back(make_snapshot("v1", "r1"));

        // selected_path が None のため undo_all は何もしない
        let result = state.undo_all();
        assert!(!result, "selected_path が None のため false を返すべき");
        assert_eq!(state.undo_stack.len(), 2, "スタックが保持されるべき");
    }

    // ── undo_all: selected_path = Some(...) のとき正常に undo される ──

    #[test]
    fn test_undo_all_with_selected_path_restores_initial() {
        let mut state = make_state();
        state.selected_path = Some("foo.rs".to_string());

        // 初期スナップショット（最前）と変更後スナップショット（最後）をプッシュ
        state
            .undo_stack
            .push_back(make_snapshot("initial", "remote_initial"));
        state.undo_stack.push_back(make_snapshot("v1", "remote_v1"));
        state.undo_stack.push_back(make_snapshot("v2", "remote_v2"));

        let result = state.undo_all();
        assert!(result, "undo_all は true を返すべき");
        // スタックが空になっていることを確認（pop_front + clear の結果）
        assert!(
            state.undo_stack.is_empty(),
            "undo_all 後はスタックが空になるべき"
        );
        // status_message が設定されていることを確認
        assert_eq!(state.status_message, "All changes undone");
    }

    // ── undo_all: 空スタックで安全に動作する ──

    #[test]
    fn test_undo_all_empty_stack_is_safe() {
        let mut state = make_state();
        state.selected_path = Some("bar.rs".to_string());

        let result = state.undo_all();
        assert!(!result, "空スタックでは false を返すべき");
        assert_eq!(state.status_message, "Nothing to undo");
    }

    // ── undo_last: 基本動作 ──

    #[test]
    fn test_undo_last_returns_false_when_empty() {
        let mut state = make_state();
        let result = state.undo_last();
        assert!(!result);
        assert_eq!(state.status_message, "Nothing to undo");
    }

    #[test]
    fn test_undo_last_returns_false_when_no_selected_path() {
        let mut state = make_state();
        state.selected_path = None;
        state.undo_stack.push_back(make_snapshot("v0", "r0"));

        let result = state.undo_last();
        // スタックから pop したが selected_path が None なので false
        assert!(!result);
        // スタックが空になっていることを確認（pop_back 済み）
        assert!(state.undo_stack.is_empty());
    }
}
