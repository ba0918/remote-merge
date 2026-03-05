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
