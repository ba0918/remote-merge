//! 2カラム diff パネルの描画。
//! 追加行（緑）、削除行（赤）、コンテキスト行の色分け表示。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::{AppState, Focus};
use crate::diff::engine::{DiffLine, DiffResult, DiffTag};

/// diff ビューウィジェット
pub struct DiffView<'a> {
    state: &'a AppState,
}

impl<'a> DiffView<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// 行番号をフォーマット（5桁右寄せ）
    fn format_line_num(num: Option<usize>) -> String {
        match num {
            Some(n) => format!("{:>5}", n + 1), // 1-indexed
            None => "     ".to_string(),
        }
    }

    /// diff タグに応じたスタイルを返す
    fn line_style(tag: DiffTag) -> Style {
        match tag {
            DiffTag::Equal => Style::default().fg(Color::White),
            DiffTag::Insert => Style::default().fg(Color::Green),
            DiffTag::Delete => Style::default().fg(Color::Red),
        }
    }

    /// diff タグのプレフィックス文字
    fn tag_char(tag: DiffTag) -> &'static str {
        match tag {
            DiffTag::Equal => " ",
            DiffTag::Insert => "+",
            DiffTag::Delete => "-",
        }
    }

    /// diff 行を Line に変換
    fn render_diff_line(line: &DiffLine) -> Line<'static> {
        let style = Self::line_style(line.tag);
        let old_num = Self::format_line_num(line.old_index);
        let new_num = Self::format_line_num(line.new_index);
        let prefix = Self::tag_char(line.tag);

        // 行末の改行を除去
        let value = line.value.trim_end_matches('\n').to_string();

        let num_style = Style::default().fg(Color::DarkGray);
        let prefix_style = match line.tag {
            DiffTag::Equal => Style::default().fg(Color::DarkGray),
            DiffTag::Insert => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            DiffTag::Delete => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        };

        Line::from(vec![
            Span::styled(old_num, num_style),
            Span::raw(" "),
            Span::styled(new_num, num_style),
            Span::raw(" "),
            Span::styled(prefix.to_string(), prefix_style),
            Span::raw(" "),
            Span::styled(value, style),
        ])
    }
}

impl<'a> Widget for DiffView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.focus == Focus::DiffView;

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let title = match &self.state.selected_path {
            Some(path) => format!(" {} ", path),
            None => " Diff ".to_string(),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        match &self.state.current_diff {
            None => {
                let msg = Paragraph::new("  Select a file to view diff");
                msg.render(inner, buf);
            }
            Some(DiffResult::Equal) => {
                let msg = Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("Files are identical", Style::default().fg(Color::Green)),
                ]));
                msg.render(inner, buf);
            }
            Some(DiffResult::Binary) => {
                let msg = Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        "Binary file - diff not available",
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
                msg.render(inner, buf);
            }
            Some(DiffResult::Modified { lines, stats, .. }) => {
                let visible_height = inner.height as usize;
                let scroll = self.state.diff_scroll.min(lines.len().saturating_sub(1));

                let mut display_lines: Vec<Line> = lines
                    .iter()
                    .skip(scroll)
                    .take(visible_height.saturating_sub(1)) // 最終行にサマリー表示
                    .map(Self::render_diff_line)
                    .collect();

                // サマリー行
                let summary = Line::from(vec![
                    Span::styled(
                        format!(
                            " +{} -{} ={} | {}/{}",
                            stats.insertions,
                            stats.deletions,
                            stats.equal,
                            scroll + 1,
                            lines.len(),
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                display_lines.push(summary);

                let paragraph = Paragraph::new(display_lines);
                paragraph.render(inner, buf);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::diff::engine::{DiffStats, DiffResult};
    use crate::tree::FileTree;
    use std::path::PathBuf;

    fn make_test_state_with_diff(diff: Option<DiffResult>) -> AppState {
        let mut state = AppState::new(
            FileTree { root: PathBuf::from("/test"), nodes: vec![] },
            FileTree { root: PathBuf::from("/test"), nodes: vec![] },
            "develop".to_string(),
        );
        state.current_diff = diff;
        state.selected_path = Some("test.txt".to_string());
        state
    }

    fn render_to_string(state: &AppState, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        let widget = DiffView::new(state);
        widget.render(area, &mut buf);

        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf.cell((x, y)).map(|c| c.symbol().to_string()).unwrap_or_default())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_no_diff_message() {
        let state = make_test_state_with_diff(None);
        let content = render_to_string(&state, 60, 10);
        assert!(content.contains("Select a file"), "ガイドメッセージが表示されるべき");
    }

    #[test]
    fn test_diff_color_lines() {
        let lines = vec![
            DiffLine { tag: DiffTag::Equal,  value: "same\n".to_string(), old_index: Some(0), new_index: Some(0) },
            DiffLine { tag: DiffTag::Delete, value: "old\n".to_string(),  old_index: Some(1), new_index: None },
            DiffLine { tag: DiffTag::Insert, value: "new\n".to_string(),  old_index: None,    new_index: Some(1) },
            DiffLine { tag: DiffTag::Equal,  value: "end\n".to_string(),  old_index: Some(2), new_index: Some(2) },
        ];

        let diff = DiffResult::Modified {
            hunks: vec![],
            lines,
            stats: DiffStats { insertions: 1, deletions: 1, equal: 2 },
        };

        let state = make_test_state_with_diff(Some(diff));
        let content = render_to_string(&state, 80, 15);

        // 行内容が描画されていること
        assert!(content.contains("same"), "Equal行が表示されるべき");
        assert!(content.contains("old"), "Delete行が表示されるべき");
        assert!(content.contains("new"), "Insert行が表示されるべき");
        assert!(content.contains("+1"), "統計が表示されるべき");
    }

    #[test]
    fn test_binary_diff_display() {
        let state = make_test_state_with_diff(Some(DiffResult::Binary));
        let content = render_to_string(&state, 60, 10);
        assert!(content.contains("Binary"), "バイナリメッセージが表示されるべき");
    }
}
