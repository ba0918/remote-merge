//! 2カラム diff パネルの描画。
//! パレット経由の色管理 + シンタックスハイライト対応。

mod content_render;
mod line_render;
mod search;
mod style_utils;
mod three_way_badge;

// 公開API（外部から使うもの）
pub use line_render::{render_diff_line_highlighted, split_for_side_by_side};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::{AppState, Focus};
use crate::diff::engine::DiffResult;

/// diff ビューウィジェット
pub struct DiffView<'a> {
    state: &'a AppState,
}

impl<'a> DiffView<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

impl<'a> Widget for DiffView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.focus == Focus::DiffView;
        let p = &self.state.palette;

        let border_style = if is_focused {
            Style::default().fg(p.border_focused)
        } else {
            Style::default().fg(p.border_unfocused)
        };

        let title = if self.state.showing_ref_diff {
            let left_name = self.state.left_source.display_name();
            let ref_name = self.state.ref_server_name().unwrap_or("ref");
            match &self.state.selected_path {
                Some(path) => format!(
                    " {} | {} ↔ {} (ref) | read-only | X: swap to merge ",
                    path, left_name, ref_name
                ),
                None => format!(
                    " Diff: {} ↔ {} (ref) | read-only | X: swap to merge ",
                    left_name, ref_name
                ),
            }
        } else {
            match &self.state.selected_path {
                Some(path) => format!(" {} ", path),
                None => " Diff ".to_string(),
            }
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(Style::default().bg(p.bg));

        let full_inner = block.inner(area);
        block.render(area, buf);

        // メタデータ行を描画し、残りの領域を返す
        let inner = self.render_metadata_line(full_inner, buf);

        match &self.state.current_diff {
            None => {
                let msg = Paragraph::new("  Select a file to view diff");
                msg.render(inner, buf);
            }
            Some(DiffResult::Equal) => {
                self.render_equal(inner, buf, is_focused);
            }
            Some(DiffResult::Binary { left, right }) => {
                self.render_binary(inner, buf, left, right);
            }
            Some(DiffResult::SymlinkDiff {
                left_target,
                right_target,
            }) => {
                self.render_symlink_diff(inner, buf, left_target, right_target);
            }
            Some(DiffResult::Modified {
                hunks: _,
                merge_hunks,
                lines,
                stats,
                ..
            }) => {
                self.render_modified(inner, buf, is_focused, merge_hunks, lines, stats);
            }
        }
    }
}
