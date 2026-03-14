//! ハンクマージ、ハンクカーソル操作。

use crate::diff::engine::{self, DiffResult, HunkDirection};

use super::types::CacheSnapshot;
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
        if let Some(DiffResult::Modified { merge_hunks, .. }) = &self.current_diff {
            if merge_hunks.is_empty() {
                return;
            }
            let indices: Vec<usize> = merge_hunks.iter().map(|h| h.line_range.start).collect();
            let pos = indices.partition_point(|&idx| idx <= self.diff_cursor);
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
        if let Some(DiffResult::Modified { merge_hunks, .. }) = &self.current_diff {
            if let Some(hunk) = merge_hunks.get(self.hunk_cursor) {
                if !hunk.is_empty() {
                    self.diff_cursor = hunk.line_range.start;
                    self.ensure_cursor_visible();
                }
            }
        }
    }

    /// ハンクマージ後にシンタックスハイライトキャッシュを再構築する。
    ///
    /// キャッシュ内容が変わった側のハイライトを破棄して再構築する。
    fn invalidate_highlight_cache(&mut self, path: &str, direction: HunkDirection) {
        match direction {
            HunkDirection::RightToLeft => {
                self.highlight_cache_left.remove(path);
            }
            HunkDirection::LeftToRight => {
                self.highlight_cache_right.remove(path);
            }
        }
        self.build_highlight_cache(path);
    }

    /// ハンクマージのプレビューテキスト（before/after）を生成する
    pub fn preview_hunk_merge(&self, direction: HunkDirection) -> Option<(String, String)> {
        let path = self.selected_path.as_ref()?;

        let (hunk_lines_cloned, old_start, new_start) = match &self.current_diff {
            Some(DiffResult::Modified {
                merge_hunks, lines, ..
            }) => {
                let hunk = merge_hunks.get(self.hunk_cursor)?;
                (hunk.lines(lines).to_vec(), hunk.old_start, hunk.new_start)
            }
            _ => return None,
        };

        let original = match direction {
            HunkDirection::RightToLeft => self.left_cache.get(path)?.clone(),
            HunkDirection::LeftToRight => self.right_cache.get(path)?.clone(),
        };

        let new_text = engine::apply_hunk_to_text(
            &original,
            &hunk_lines_cloned,
            old_start,
            new_start,
            direction,
        );
        Some((original, new_text))
    }

    /// ハンク単位マージを実行する（即時適用 + undo スナップショット保存）
    pub fn apply_hunk_merge(&mut self, direction: HunkDirection) -> Option<String> {
        let path = self.selected_path.clone()?;

        // DiffResult から必要なデータを事前にクローンして借用を解消
        let (hunk_lines_cloned, old_start, new_start) = match &self.current_diff {
            Some(DiffResult::Modified {
                merge_hunks, lines, ..
            }) => {
                let hunk = merge_hunks.get(self.hunk_cursor)?;
                (hunk.lines(lines).to_vec(), hunk.old_start, hunk.new_start)
            }
            _ => return None,
        };

        // undo 用スナップショットを保存
        let local_content = self.left_cache.get(&path)?.clone();
        let remote_content = self.right_cache.get(&path)?.clone();

        // 適用先テキストを先に取得（clone 1回で済ませるため snapshot push 前に分岐）
        let original = match direction {
            HunkDirection::RightToLeft => local_content.clone(),
            HunkDirection::LeftToRight => remote_content.clone(),
        };

        // snapshot は所有権移動で clone 不要
        self.push_undo_snapshot(CacheSnapshot {
            local_content,
            remote_content,
        });

        let new_text = engine::apply_hunk_to_text(
            &original,
            &hunk_lines_cloned,
            old_start,
            new_start,
            direction,
        );

        // キャッシュを更新
        match direction {
            HunkDirection::RightToLeft => {
                self.left_cache.insert(path.clone(), new_text.clone());
            }
            HunkDirection::LeftToRight => {
                self.right_cache.insert(path.clone(), new_text.clone());
            }
        }

        // シンタックスハイライトキャッシュを再構築
        // （キャッシュ内容が変わったので古いハイライト結果を破棄する）
        self.invalidate_highlight_cache(&path, direction);

        // diff を再計算
        let local = self.left_cache.get(&path);
        let remote = self.right_cache.get(&path);
        if let (Some(l), Some(r)) = (local, remote) {
            self.current_diff = Some(engine::compute_diff(l, r));
        }

        // ハンクカーソルを範囲内に収める
        self.clamp_hunk_cursor();

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, FlatNode, MAX_UNDO_STACK};
    use crate::app::Side;
    use crate::diff::engine::{self, DiffHunk};
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_test_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_state() -> AppState {
        AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        )
    }

    /// diff付きstateを作成するヘルパー
    fn make_state_with_diff(local: &str, remote: &str) -> AppState {
        let node = FileNode::new_file("a.rs");
        let mut state = AppState::new(
            make_test_tree(vec![node.clone()]),
            make_test_tree(vec![node]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.flat_nodes = vec![FlatNode {
            path: "a.rs".to_string(),
            name: "a.rs".to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Modified,
            ref_only: false,
        }];
        state
            .left_cache
            .insert("a.rs".to_string(), local.to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), remote.to_string());
        state.selected_path = Some("a.rs".to_string());
        state.current_diff = Some(engine::compute_diff(local, remote));
        state
    }

    #[test]
    fn test_hunk_count_none() {
        let state = make_state();
        assert_eq!(state.hunk_count(), 0);
    }

    #[test]
    fn test_hunk_count_equal() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Equal);
        assert_eq!(state.hunk_count(), 0);
    }

    #[test]
    fn test_hunk_count_modified() {
        let state = make_state_with_diff("line1\nline2\n", "line1\nchanged\n");
        assert!(state.hunk_count() > 0);
    }

    #[test]
    fn test_stage_hunk_merge_no_hunks() {
        let mut state = make_state();
        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert!(state.pending_hunk_merge.is_none());
    }

    #[test]
    fn test_stage_hunk_merge() {
        let mut state = make_state_with_diff("line1\nline2\n", "line1\nchanged\n");
        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert_eq!(state.pending_hunk_merge, Some(HunkDirection::RightToLeft));
        assert!(state.status_message.contains("remote -> local"));
    }

    #[test]
    fn test_stage_hunk_merge_left_to_right() {
        let mut state = make_state_with_diff("line1\nline2\n", "line1\nchanged\n");
        state.stage_hunk_merge(HunkDirection::LeftToRight);
        assert_eq!(state.pending_hunk_merge, Some(HunkDirection::LeftToRight));
        assert!(state.status_message.contains("local -> remote"));
    }

    #[test]
    fn test_cancel_hunk_merge_when_pending() {
        let mut state = make_state_with_diff("a\n", "b\n");
        state.pending_hunk_merge = Some(HunkDirection::RightToLeft);
        state.cancel_hunk_merge();
        assert!(state.pending_hunk_merge.is_none());
        assert!(state.status_message.contains("cancelled"));
    }

    #[test]
    fn test_cancel_hunk_merge_when_none() {
        let mut state = make_state();
        let old_msg = state.status_message.clone();
        state.cancel_hunk_merge();
        // 何も変わらない
        assert_eq!(state.status_message, old_msg);
    }

    #[test]
    fn test_has_unsaved_changes_empty() {
        let state = make_state();
        assert!(!state.has_unsaved_changes());
    }

    #[test]
    fn test_has_unsaved_changes_after_apply() {
        let mut state = make_state_with_diff("line1\nline2\n", "line1\nchanged\n");
        if state.hunk_count() > 0 {
            state.apply_hunk_merge(HunkDirection::RightToLeft);
            assert!(state.has_unsaved_changes());
        }
    }

    #[test]
    fn test_apply_hunk_merge() {
        let mut state = make_state_with_diff("line1\nold\nline3\n", "line1\nnew\nline3\n");
        let count_before = state.hunk_count();
        assert!(count_before > 0);

        let result = state.apply_hunk_merge(HunkDirection::RightToLeft);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "a.rs");
        assert!(!state.undo_stack.is_empty());
        assert!(state.status_message.contains("applied"));
    }

    #[test]
    fn test_apply_hunk_merge_no_diff() {
        let mut state = make_state();
        state.selected_path = Some("a.rs".to_string());
        let result = state.apply_hunk_merge(HunkDirection::RightToLeft);
        assert!(result.is_none());
    }

    #[test]
    fn test_undo_last() {
        let mut state = make_state_with_diff("line1\nold\n", "line1\nnew\n");
        state.apply_hunk_merge(HunkDirection::RightToLeft);
        assert!(state.has_unsaved_changes());

        let result = state.undo_last();
        assert!(result);
        assert!(!state.has_unsaved_changes());
        assert!(state.status_message.contains("Undo"));
    }

    #[test]
    fn test_undo_last_empty_stack() {
        let mut state = make_state();
        let result = state.undo_last();
        assert!(!result);
        assert!(state.status_message.contains("Nothing to undo"));
    }

    #[test]
    fn test_undo_all() {
        let mut state = make_state_with_diff("a\nb\nc\n", "a\nx\ny\n");
        // 複数回適用
        state.apply_hunk_merge(HunkDirection::RightToLeft);
        if state.hunk_count() > 0 {
            state.apply_hunk_merge(HunkDirection::RightToLeft);
        }

        let result = state.undo_all();
        assert!(result);
        assert!(!state.has_unsaved_changes());
        assert!(state.status_message.contains("undone"));
    }

    #[test]
    fn test_undo_all_empty_stack() {
        let mut state = make_state();
        let result = state.undo_all();
        assert!(!result);
        assert!(state.status_message.contains("Nothing to undo"));
    }

    #[test]
    fn test_hunk_cursor_up() {
        let mut state = make_state_with_diff("a\nb\nc\n", "x\ny\nz\n");
        state.hunk_cursor = 1;
        state.hunk_cursor_up();
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_hunk_cursor_up_at_zero() {
        let mut state = make_state_with_diff("a\n", "b\n");
        state.hunk_cursor = 0;
        state.hunk_cursor_up();
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_hunk_cursor_down() {
        let mut state = make_state_with_diff("a\nb\nc\n", "x\ny\nz\n");
        let count = state.hunk_count();
        if count > 1 {
            state.hunk_cursor = 0;
            state.hunk_cursor_down();
            assert_eq!(state.hunk_cursor, 1);
        }
    }

    #[test]
    fn test_hunk_cursor_down_at_end() {
        let mut state = make_state_with_diff("a\n", "b\n");
        let count = state.hunk_count();
        state.hunk_cursor = count.saturating_sub(1);
        state.hunk_cursor_down();
        assert_eq!(state.hunk_cursor, count.saturating_sub(1));
    }

    #[test]
    fn test_preview_hunk_merge() {
        let state = make_state_with_diff("line1\nold\nline3\n", "line1\nnew\nline3\n");
        let preview = state.preview_hunk_merge(HunkDirection::RightToLeft);
        assert!(preview.is_some());
        let (before, after) = preview.unwrap();
        assert!(before.contains("old"));
        assert!(after.contains("new"));
    }

    #[test]
    fn test_preview_hunk_merge_no_diff() {
        let mut state = make_state();
        state.selected_path = Some("a.rs".to_string());
        let preview = state.preview_hunk_merge(HunkDirection::RightToLeft);
        assert!(preview.is_none());
    }

    #[test]
    fn test_sync_hunk_cursor_to_scroll() {
        let mut state = make_state_with_diff("a\nb\nc\n", "x\ny\nz\n");
        state.diff_cursor = 0;
        state.sync_hunk_cursor_to_scroll();
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_scroll_to_hunk_uses_content_based_search() {
        // ハンクがクローンされた場合でもスクロールが正しく動作することを確認
        let mut state = make_state_with_diff("a\nb\nc\n", "a\nx\nc\n");
        assert!(state.hunk_count() > 0);

        // hunk_cursor_down / hunk_cursor_up は内部で scroll_to_hunk を呼ぶ
        // クローンされたハンクでもパニックせず正しく動作する
        state.hunk_cursor = 0;
        state.hunk_cursor_down();
        // カーソルが範囲外でなければOK（ハンクが1つの場合は0のまま）
        assert!(state.hunk_cursor <= state.hunk_count().saturating_sub(1));

        // 複数ハンクのケース
        let mut state2 = make_state_with_diff("a\nb\nc\nd\ne\n", "a\nx\nc\ny\ne\n");
        let count = state2.hunk_count();
        if count > 1 {
            state2.hunk_cursor = 0;
            state2.hunk_cursor_down();
            assert_eq!(state2.hunk_cursor, 1);
            // diff_cursor がハンク開始位置に移動していることを確認
            assert!(state2.diff_cursor > 0);

            state2.hunk_cursor_up();
            assert_eq!(state2.hunk_cursor, 0);
        }
    }

    #[test]
    fn test_empty_hunk_is_empty() {
        let empty_hunk = DiffHunk {
            line_range: 0..0,
            old_start: 0,
            new_start: 0,
        };
        assert!(empty_hunk.is_empty());
        assert_eq!(empty_hunk.len(), 0);
    }

    #[test]
    fn test_undo_stack_max_limit() {
        let mut state = make_state_with_diff("a\n", "b\n");
        // ちょうど MAX_UNDO_STACK 個入れる
        for _ in 0..MAX_UNDO_STACK {
            state.undo_stack.push_back(CacheSnapshot {
                local_content: "x".to_string(),
                remote_content: "y".to_string(),
            });
        }
        assert_eq!(state.undo_stack.len(), MAX_UNDO_STACK);
        // apply で pop_front + push_back → 個数は MAX_UNDO_STACK のまま
        if state.hunk_count() > 0 {
            state.apply_hunk_merge(HunkDirection::RightToLeft);
            assert_eq!(state.undo_stack.len(), MAX_UNDO_STACK);
        }
    }

    #[test]
    fn test_undo_last_recomputes_diff() {
        // apply_hunk_merge で変更 → undo_last → diff が元の状態に再計算されること
        let mut state = make_state_with_diff("line1\nold\n", "line1\nnew\n");
        let hunk_count_before = state.hunk_count();
        assert!(hunk_count_before > 0);

        state.apply_hunk_merge(HunkDirection::RightToLeft);
        // マージ後は diff が変わっている（ハンクが減る = Equal になる）
        assert_ne!(state.hunk_count(), hunk_count_before);

        // undo → diff が元の状態に再計算される
        state.undo_last();
        assert_eq!(state.hunk_count(), hunk_count_before);
    }

    #[test]
    fn test_undo_all_recomputes_diff() {
        let mut state = make_state_with_diff("a\nb\nc\n", "a\nx\nc\n");
        let original_hunk_count = state.hunk_count();
        assert!(original_hunk_count > 0);

        state.apply_hunk_merge(HunkDirection::RightToLeft);
        // マージ後はハンクが減っている
        assert_ne!(state.hunk_count(), original_hunk_count);

        state.undo_all();
        // undo_all → 初期状態の diff が再計算される
        assert_eq!(state.hunk_count(), original_hunk_count);
    }

    #[test]
    fn test_apply_undo_roundtrip_preserves_content() {
        let left = "line1\nold\nline3\n";
        let right = "line1\nnew\nline3\n";
        let mut state = make_state_with_diff(left, right);

        // apply
        state.apply_hunk_merge(HunkDirection::RightToLeft);
        // undo
        state.undo_last();

        // キャッシュの内容が元に戻っていること
        assert_eq!(state.left_cache.get("a.rs").unwrap(), left);
        assert_eq!(state.right_cache.get("a.rs").unwrap(), right);
        // diff も正しく再計算されていること
        assert!(state.current_diff.is_some());
        assert!(state.hunk_count() > 0);
    }

    #[test]
    fn test_apply_hunk_snapshot_stores_original_content() {
        // apply_hunk_merge のスナップショットが元のコンテンツを正しく保持していることを確認
        // （クローン削減後も所有権移動で正しくスナップショットに保存される）
        let left = "line1\noriginal_left\nline3\n";
        let right = "line1\noriginal_right\nline3\n";
        let mut state = make_state_with_diff(left, right);

        state.apply_hunk_merge(HunkDirection::RightToLeft);
        assert_eq!(state.undo_stack.len(), 1);

        let snapshot = state.undo_stack.back().unwrap();
        assert_eq!(snapshot.local_content, left);
        assert_eq!(snapshot.remote_content, right);
    }

    #[test]
    fn test_apply_hunk_snapshot_left_to_right() {
        let left = "aaa\nbbb\n";
        let right = "aaa\nccc\n";
        let mut state = make_state_with_diff(left, right);

        state.apply_hunk_merge(HunkDirection::LeftToRight);
        assert_eq!(state.undo_stack.len(), 1);

        let snapshot = state.undo_stack.back().unwrap();
        assert_eq!(snapshot.local_content, left);
        assert_eq!(snapshot.remote_content, right);
    }

    #[test]
    fn test_undo_last_with_no_selected_path() {
        let mut state = make_state_with_diff("a\n", "b\n");
        state.apply_hunk_merge(HunkDirection::RightToLeft);
        assert_eq!(state.undo_stack.len(), 1);

        // selected_path を None にする
        state.selected_path = None;
        let result = state.undo_last();
        // pop_back は実行されるがパス不明で復元不可
        assert!(!result);
        assert!(state.undo_stack.is_empty());
    }
}
