//! ハンクマージ、undo、ハンクカーソル操作。

use crate::diff::engine::{self, DiffHunk, DiffLine, DiffResult, HunkDirection};

use super::types::{CacheSnapshot, MAX_UNDO_STACK};
use super::AppState;

impl AppState {
    /// ハンクマージの保留をセットする（→/← で呼ぶ）
    pub fn stage_hunk_merge(&mut self, direction: HunkDirection) {
        if self.hunk_count() == 0 {
            return;
        }
        self.pending_hunk_merge = Some(direction);
        let dir_str = match direction {
            HunkDirection::RightToLeft => "remote -> local",
            HunkDirection::LeftToRight => "local -> remote",
        };
        self.status_message = format!(
            "Hunk {}/{} ({}) -- Enter: apply / Esc: cancel",
            self.hunk_cursor + 1,
            self.hunk_count(),
            dir_str,
        );
    }

    /// 保留中のハンクマージをキャンセルする
    pub fn cancel_hunk_merge(&mut self) {
        if self.pending_hunk_merge.is_some() {
            self.pending_hunk_merge = None;
            self.status_message = format!(
                "Hunk merge cancelled | hunk {}/{}",
                self.hunk_cursor + 1,
                self.hunk_count(),
            );
        }
    }

    /// 現在の diff のハンク数を返す
    pub fn hunk_count(&self) -> usize {
        match &self.current_diff {
            Some(DiffResult::Modified { merge_hunks, .. }) => merge_hunks.len(),
            _ => 0,
        }
    }

    /// カーソル位置に最も近いハンクにハンクカーソルを同期する（二分探索）
    pub fn sync_hunk_cursor_to_scroll(&mut self) {
        if let Some(DiffResult::Modified {
            merge_hunk_line_indices,
            ..
        }) = &self.current_diff
        {
            if merge_hunk_line_indices.is_empty() {
                return;
            }
            let pos = merge_hunk_line_indices.partition_point(|&idx| idx <= self.diff_cursor);
            self.hunk_cursor = if pos == 0 { 0 } else { pos - 1 };
        }
    }

    /// ハンクカーソルを上に移動（前のハンクへ）
    pub fn hunk_cursor_up(&mut self) {
        if self.hunk_cursor > 0 {
            self.hunk_cursor -= 1;
            self.scroll_to_hunk();
        }
    }

    /// ハンクカーソルを下に移動（次のハンクへ）
    pub fn hunk_cursor_down(&mut self) {
        let count = self.hunk_count();
        if count > 0 && self.hunk_cursor + 1 < count {
            self.hunk_cursor += 1;
            self.scroll_to_hunk();
        }
    }

    /// ハンクカーソル位置に diff_cursor を合わせ、ビューポートも追従させる
    fn scroll_to_hunk(&mut self) {
        if let Some(DiffResult::Modified {
            merge_hunks, lines, ..
        }) = &self.current_diff
        {
            if let Some(hunk) = merge_hunks.get(self.hunk_cursor) {
                if let Some(first_hunk_line) = hunk.lines.first() {
                    let target = lines
                        .iter()
                        .position(|l| std::ptr::eq(l, first_hunk_line))
                        .unwrap_or_else(|| self.find_hunk_start_in_lines(lines, hunk));
                    self.diff_cursor = target;
                    self.ensure_cursor_visible();
                }
            }
        }
    }

    /// ハンクの開始位置を lines 内で探す（内容ベース）
    fn find_hunk_start_in_lines(&self, lines: &[DiffLine], hunk: &DiffHunk) -> usize {
        if hunk.lines.is_empty() {
            return 0;
        }
        let first = &hunk.lines[0];
        for (i, line) in lines.iter().enumerate() {
            if line.tag == first.tag
                && line.value == first.value
                && line.old_index == first.old_index
                && line.new_index == first.new_index
            {
                return i;
            }
        }
        0
    }

    /// ハンクマージのプレビューテキスト（before/after）を生成する
    pub fn preview_hunk_merge(&self, direction: HunkDirection) -> Option<(String, String)> {
        let path = self.selected_path.as_ref()?;

        let hunks = match &self.current_diff {
            Some(DiffResult::Modified { merge_hunks, .. }) => merge_hunks.clone(),
            _ => return None,
        };

        let hunk = hunks.get(self.hunk_cursor)?;

        let original = match direction {
            HunkDirection::RightToLeft => self.local_cache.get(path)?.clone(),
            HunkDirection::LeftToRight => self.remote_cache.get(path)?.clone(),
        };

        let new_text = engine::apply_hunk_to_text(&original, hunk, direction);
        Some((original, new_text))
    }

    /// ハンク単位マージを実行する（即時適用 + undo スナップショット保存）
    pub fn apply_hunk_merge(&mut self, direction: HunkDirection) -> Option<String> {
        let path = self.selected_path.clone()?;

        let hunks = match &self.current_diff {
            Some(DiffResult::Modified { merge_hunks, .. }) => merge_hunks.clone(),
            _ => return None,
        };

        let hunk = hunks.get(self.hunk_cursor)?;

        // undo 用スナップショットを保存
        let local_content = self.local_cache.get(&path)?.clone();
        let remote_content = self.remote_cache.get(&path)?.clone();
        if self.undo_stack.len() >= MAX_UNDO_STACK {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(CacheSnapshot {
            local_content: local_content.clone(),
            remote_content: remote_content.clone(),
            diff: self.current_diff.clone(),
        });

        // 適用先テキストを取得
        let original = match direction {
            HunkDirection::RightToLeft => local_content,
            HunkDirection::LeftToRight => remote_content,
        };

        let new_text = engine::apply_hunk_to_text(&original, hunk, direction);

        // キャッシュを更新
        match direction {
            HunkDirection::RightToLeft => {
                self.local_cache.insert(path.clone(), new_text.clone());
            }
            HunkDirection::LeftToRight => {
                self.remote_cache.insert(path.clone(), new_text.clone());
            }
        }

        // diff を再計算
        let local = self.local_cache.get(&path);
        let remote = self.remote_cache.get(&path);
        if let (Some(l), Some(r)) = (local, remote) {
            self.current_diff = Some(engine::compute_diff(l, r));
        }

        // ハンクカーソルを範囲内に収める
        let new_count = self.hunk_count();
        if new_count == 0 {
            self.hunk_cursor = 0;
        } else if self.hunk_cursor >= new_count {
            self.hunk_cursor = new_count - 1;
        }

        // バッジを再構築
        self.rebuild_flat_nodes();

        let dir_str = match direction {
            HunkDirection::RightToLeft => "right -> left",
            HunkDirection::LeftToRight => "left -> right",
        };
        self.status_message = format!(
            "Hunk applied ({}) | {} changes | w:write u:undo",
            dir_str,
            self.undo_stack.len(),
        );

        Some(path)
    }

    /// 最後のハンク操作を undo する
    pub fn undo_last(&mut self) -> bool {
        if let Some(snapshot) = self.undo_stack.pop_back() {
            if let Some(path) = &self.selected_path {
                self.local_cache
                    .insert(path.clone(), snapshot.local_content);
                self.remote_cache
                    .insert(path.clone(), snapshot.remote_content);
                self.current_diff = snapshot.diff;

                let new_count = self.hunk_count();
                if new_count == 0 {
                    self.hunk_cursor = 0;
                } else if self.hunk_cursor >= new_count {
                    self.hunk_cursor = new_count - 1;
                }

                self.rebuild_flat_nodes();
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

        if let Some(path) = &self.selected_path {
            self.local_cache.insert(path.clone(), initial.local_content);
            self.remote_cache
                .insert(path.clone(), initial.remote_content);
            self.current_diff = initial.diff;

            let new_count = self.hunk_count();
            if new_count == 0 {
                self.hunk_cursor = 0;
            } else if self.hunk_cursor >= new_count {
                self.hunk_cursor = new_count - 1;
            }

            self.rebuild_flat_nodes();
            self.status_message = "All changes undone".to_string();
            return true;
        }
        false
    }

    /// 未保存の変更があるかどうか
    pub fn has_unsaved_changes(&self) -> bool {
        !self.undo_stack.is_empty()
    }
}
