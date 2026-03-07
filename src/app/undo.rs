//! Undo スタック管理。
//!
//! ハンクマージ操作の undo/redo を管理する。
//! CacheSnapshot のスタック操作と AppState への復元を行う。

use super::types::{CacheSnapshot, MAX_UNDO_STACK};
use super::AppState;

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
    pub fn undo_all(&mut self) -> bool {
        if self.undo_stack.is_empty() {
            self.status_message = "Nothing to undo".to_string();
            return false;
        }

        let initial = self
            .undo_stack
            .pop_front()
            .expect("undo_stack is not empty");
        self.undo_stack.clear();

        if let Some(path) = self.selected_path.clone() {
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
    fn restore_snapshot(&mut self, snapshot: CacheSnapshot, path: &str) {
        self.left_cache
            .insert(path.to_string(), snapshot.local_content);
        self.right_cache
            .insert(path.to_string(), snapshot.remote_content);
        self.current_diff = snapshot.diff;

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
