//! カーソル移動、スクロール、ページ送り。

use crate::diff::engine::DiffResult;

use super::AppState;

/// スクロールマージン（上下端からこの行数を残してスクロールを開始する）
const SCROLL_MARGIN: usize = 3;

impl AppState {
    /// フォーカスを切り替える (Tab)
    pub fn toggle_focus(&mut self) {
        use super::types::Focus;
        self.focus = match self.focus {
            Focus::FileTree => Focus::DiffView,
            Focus::DiffView => Focus::FileTree,
        };
    }

    /// ツリーカーソルを上に移動
    pub fn cursor_up(&mut self) {
        if self.tree_cursor > 0 {
            self.tree_cursor -= 1;
            self.ensure_tree_cursor_visible();
        }
    }

    /// ツリーカーソルを下に移動
    pub fn cursor_down(&mut self) {
        if self.tree_cursor + 1 < self.flat_nodes.len() {
            self.tree_cursor += 1;
            self.ensure_tree_cursor_visible();
        }
    }

    /// ツリーカーソルの可視範囲を保証する（スクロールマージン付き）
    pub fn ensure_tree_cursor_visible(&mut self) {
        let height = self.tree_visible_height.max(1);
        let total = self.flat_nodes.len();
        let margin = SCROLL_MARGIN.min(height / 2);

        if total <= height {
            self.tree_scroll = 0;
            return;
        }

        // カーソルが上端マージンより上にある場合
        if self.tree_cursor < self.tree_scroll.saturating_add(margin) {
            self.tree_scroll = self.tree_cursor.saturating_sub(margin);
        }

        // カーソルが下端マージンより下にある場合
        if self.tree_cursor + margin >= self.tree_scroll + height {
            self.tree_scroll = (self.tree_cursor + margin + 1).saturating_sub(height);
        }

        // スクロール範囲のクランプ
        let max_scroll = total.saturating_sub(height);
        self.tree_scroll = self.tree_scroll.min(max_scroll);
    }

    /// diff カーソルを上に移動
    pub fn scroll_up(&mut self) {
        self.diff_cursor = self.diff_cursor.saturating_sub(1);
        self.ensure_cursor_visible();
    }

    /// diff カーソルを下に移動
    pub fn scroll_down(&mut self) {
        let max = self.diff_line_count().saturating_sub(1);
        if self.diff_cursor < max {
            self.diff_cursor += 1;
        }
        self.ensure_cursor_visible();
    }

    /// カーソル位置に応じてビューポートスクロールを調整する（VSCode準拠）
    pub fn ensure_cursor_visible(&mut self) {
        // 表示モード別の予約行数を差し引いた有効表示高さを使う
        let reserved = match &self.current_diff {
            Some(DiffResult::Equal) => 2,
            Some(DiffResult::Modified { .. }) => 1,
            _ => 0,
        };
        let height = self.diff_visible_height.saturating_sub(reserved).max(1);
        let margin = SCROLL_MARGIN.min(height / 2);
        let total = self.diff_line_count();

        // カーソルを有効範囲にクランプ
        if total > 0 {
            self.diff_cursor = self.diff_cursor.min(total - 1);
        } else {
            self.diff_cursor = 0;
            self.diff_scroll = 0;
            return;
        }

        // カーソルが上端マージンより上にある場合
        if self.diff_cursor < self.diff_scroll + margin {
            self.diff_scroll = self.diff_cursor.saturating_sub(margin);
        }

        // カーソルが下端マージンより下にある場合
        if self.diff_cursor + margin >= self.diff_scroll + height {
            self.diff_scroll = (self.diff_cursor + margin + 1).saturating_sub(height);
        }

        // スクロール範囲のクランプ
        let max_scroll = total.saturating_sub(height);
        self.diff_scroll = self.diff_scroll.min(max_scroll);
    }

    /// Diff 表示モードを切り替える (d キー)
    pub fn toggle_diff_mode(&mut self) {
        use super::types::DiffMode;
        self.diff_mode = match self.diff_mode {
            DiffMode::Unified => DiffMode::SideBySide,
            DiffMode::SideBySide => DiffMode::Unified,
        };
    }

    /// diff の全行数を返す（Equal 時はキャッシュのコンテンツ行数）
    pub fn diff_line_count(&self) -> usize {
        match &self.current_diff {
            Some(DiffResult::Modified { lines, .. }) => lines.len(),
            Some(DiffResult::Equal) => self
                .selected_path
                .as_ref()
                .and_then(|p| self.local_cache.get(p))
                .map(|c| c.lines().count())
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// フッターに表示するキーバインドヒント文字列を生成する
    pub fn build_key_hints(&self) -> String {
        use super::types::Focus;
        match self.focus {
            Focus::FileTree => "[j/k] move [Enter] open [Tab] diff [?] help".to_string(),
            Focus::DiffView => match &self.current_diff {
                Some(DiffResult::Equal) => "[j/k] scroll [Tab] tree [?] help".to_string(),
                Some(DiffResult::Modified { .. }) => {
                    if self.has_unsaved_changes() {
                        format!(
                            "[{} changes] [w] write [u] undo [→/←] apply",
                            self.undo_stack.len()
                        )
                    } else {
                        "[j/k] scroll [n/N] hunk [→/←] apply [?] help".to_string()
                    }
                }
                _ => "[Tab] tree [?] help".to_string(),
            },
        }
    }

    /// ページ下スクロール（カーソルも一緒に移動）
    pub fn scroll_page_down(&mut self, page_size: usize) {
        let max = self.diff_line_count().saturating_sub(1);
        self.diff_cursor = (self.diff_cursor + page_size).min(max);
        self.ensure_cursor_visible();
    }

    /// ページ上スクロール（カーソルも一緒に移動）
    pub fn scroll_page_up(&mut self, page_size: usize) {
        self.diff_cursor = self.diff_cursor.saturating_sub(page_size);
        self.ensure_cursor_visible();
    }

    /// 先頭にスクロール
    pub fn scroll_to_home(&mut self) {
        self.diff_cursor = 0;
        self.diff_scroll = 0;
    }

    /// 末尾にスクロール
    pub fn scroll_to_end(&mut self) {
        self.diff_cursor = self.diff_line_count().saturating_sub(1);
        self.ensure_cursor_visible();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, DiffMode, FlatNode, Focus};
    use crate::diff::engine::{DiffLine, DiffResult, DiffTag};
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
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        )
    }

    fn make_flat_nodes(n: usize) -> Vec<FlatNode> {
        (0..n)
            .map(|i| FlatNode {
                path: format!("file{}.rs", i),
                name: format!("file{}.rs", i),
                depth: 0,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::Unchecked,
            })
            .collect()
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

    #[test]
    fn test_toggle_focus() {
        let mut state = make_state();
        assert_eq!(state.focus, Focus::FileTree);
        state.toggle_focus();
        assert_eq!(state.focus, Focus::DiffView);
        state.toggle_focus();
        assert_eq!(state.focus, Focus::FileTree);
    }

    #[test]
    fn test_cursor_up_at_zero() {
        let mut state = make_state();
        state.flat_nodes = make_flat_nodes(5);
        state.tree_cursor = 0;
        state.cursor_up();
        assert_eq!(state.tree_cursor, 0);
    }

    #[test]
    fn test_cursor_up() {
        let mut state = make_state();
        state.flat_nodes = make_flat_nodes(5);
        state.tree_cursor = 3;
        state.cursor_up();
        assert_eq!(state.tree_cursor, 2);
    }

    #[test]
    fn test_cursor_down() {
        let mut state = make_state();
        state.flat_nodes = make_flat_nodes(5);
        state.tree_cursor = 2;
        state.cursor_down();
        assert_eq!(state.tree_cursor, 3);
    }

    #[test]
    fn test_cursor_down_at_end() {
        let mut state = make_state();
        state.flat_nodes = make_flat_nodes(5);
        state.tree_cursor = 4;
        state.cursor_down();
        assert_eq!(state.tree_cursor, 4);
    }

    #[test]
    fn test_ensure_tree_cursor_visible_small_tree() {
        let mut state = make_state();
        state.flat_nodes = make_flat_nodes(3);
        state.tree_visible_height = 10;
        state.tree_scroll = 5;
        state.tree_cursor = 0;
        state.ensure_tree_cursor_visible();
        assert_eq!(state.tree_scroll, 0);
    }

    #[test]
    fn test_ensure_tree_cursor_visible_scroll_down() {
        let mut state = make_state();
        state.flat_nodes = make_flat_nodes(50);
        state.tree_visible_height = 10;
        state.tree_scroll = 0;
        state.tree_cursor = 20;
        state.ensure_tree_cursor_visible();
        assert!(state.tree_scroll > 0);
    }

    #[test]
    fn test_scroll_up_diff() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(20),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        state.diff_cursor = 5;
        state.scroll_up();
        assert_eq!(state.diff_cursor, 4);
    }

    #[test]
    fn test_scroll_up_at_zero() {
        let mut state = make_state();
        state.diff_cursor = 0;
        state.scroll_up();
        assert_eq!(state.diff_cursor, 0);
    }

    #[test]
    fn test_scroll_down_diff() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(20),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        state.diff_cursor = 5;
        state.scroll_down();
        assert_eq!(state.diff_cursor, 6);
    }

    #[test]
    fn test_scroll_down_at_end() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(10),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        state.diff_cursor = 9;
        state.scroll_down();
        assert_eq!(state.diff_cursor, 9);
    }

    #[test]
    fn test_toggle_diff_mode() {
        let mut state = make_state();
        assert_eq!(state.diff_mode, DiffMode::Unified);
        state.toggle_diff_mode();
        assert_eq!(state.diff_mode, DiffMode::SideBySide);
        state.toggle_diff_mode();
        assert_eq!(state.diff_mode, DiffMode::Unified);
    }

    #[test]
    fn test_diff_line_count_none() {
        let state = make_state();
        assert_eq!(state.diff_line_count(), 0);
    }

    #[test]
    fn test_diff_line_count_modified() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(15),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        assert_eq!(state.diff_line_count(), 15);
    }

    #[test]
    fn test_diff_line_count_equal() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Equal);
        state.selected_path = Some("a.rs".to_string());
        state
            .local_cache
            .insert("a.rs".to_string(), "line1\nline2\nline3".to_string());
        assert_eq!(state.diff_line_count(), 3);
    }

    #[test]
    fn test_build_key_hints_file_tree() {
        let state = make_state();
        let hints = state.build_key_hints();
        assert!(hints.contains("move"));
        assert!(hints.contains("open"));
    }

    #[test]
    fn test_build_key_hints_diff_view_equal() {
        let mut state = make_state();
        state.focus = Focus::DiffView;
        state.current_diff = Some(DiffResult::Equal);
        let hints = state.build_key_hints();
        assert!(hints.contains("scroll"));
    }

    #[test]
    fn test_build_key_hints_diff_view_modified_no_changes() {
        let mut state = make_state();
        state.focus = Focus::DiffView;
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(5),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        let hints = state.build_key_hints();
        assert!(hints.contains("hunk"));
    }

    #[test]
    fn test_build_key_hints_diff_view_no_diff() {
        let mut state = make_state();
        state.focus = Focus::DiffView;
        state.current_diff = None;
        let hints = state.build_key_hints();
        assert!(hints.contains("tree"));
    }

    #[test]
    fn test_scroll_page_down() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(100),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        state.diff_cursor = 0;
        state.scroll_page_down(20);
        assert_eq!(state.diff_cursor, 20);
    }

    #[test]
    fn test_scroll_page_up() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(100),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        state.diff_cursor = 30;
        state.scroll_page_up(20);
        assert_eq!(state.diff_cursor, 10);
    }

    #[test]
    fn test_scroll_to_home() {
        let mut state = make_state();
        state.diff_cursor = 50;
        state.diff_scroll = 40;
        state.scroll_to_home();
        assert_eq!(state.diff_cursor, 0);
        assert_eq!(state.diff_scroll, 0);
    }

    #[test]
    fn test_scroll_to_end() {
        let mut state = make_state();
        state.current_diff = Some(DiffResult::Modified {
            hunks: vec![],
            stats: crate::diff::engine::DiffStats {
                insertions: 0,
                deletions: 0,
                equal: 0,
            },
            lines: make_diff_lines(20),
            merge_hunks: vec![],
            merge_hunk_line_indices: vec![],
        });
        state.scroll_to_end();
        assert_eq!(state.diff_cursor, 19);
    }

    #[test]
    fn test_ensure_cursor_visible_empty() {
        let mut state = make_state();
        state.current_diff = None;
        state.diff_cursor = 10;
        state.ensure_cursor_visible();
        assert_eq!(state.diff_cursor, 0);
        assert_eq!(state.diff_scroll, 0);
    }
}
