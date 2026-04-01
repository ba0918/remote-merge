//! ハンクマージプレビュー（Widget のみ。データ型は app/dialog_types.rs）。

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::dialog_types::HunkMergePreview;
use crate::theme::palette::TuiPalette;

use super::render_dialog_frame;

/// ハンクマージプレビューウィジェット
pub struct HunkMergePreviewWidget<'a> {
    preview: &'a HunkMergePreview,
    palette: &'a TuiPalette,
}

impl<'a> HunkMergePreviewWidget<'a> {
    pub fn new(preview: &'a HunkMergePreview, palette: &'a TuiPalette) -> Self {
        Self { preview, palette }
    }

    /// before/after テキストから差分がある行のみを抽出して表示用行を生成
    fn build_preview_lines(text: &str, max_lines: usize, fg: Color) -> Vec<Line<'static>> {
        text.lines()
            .take(max_lines)
            .map(|line| Line::from(Span::styled(line.to_string(), Style::default().fg(fg))))
            .collect()
    }
}

impl<'a> Widget for HunkMergePreviewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = area.width.min(76);
        let height = area.height.min(24);
        let title = format!(" Hunk Merge Preview ({}) ", self.preview.direction_label);
        let inner = render_dialog_frame(
            &title,
            self.palette.dialog_accent,
            width,
            height,
            area,
            buf,
            self.palette.bg,
        );

        let half_height = inner.height.saturating_sub(4) / 2;
        let constraints = vec![
            Constraint::Length(1),           // ファイルパス
            Constraint::Length(1),           // "Before:" ラベル
            Constraint::Length(half_height), // before テキスト
            Constraint::Length(1),           // "After:" ラベル
            Constraint::Length(half_height), // after テキスト
            Constraint::Length(1),           // ガイド
        ];
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let p = self.palette;

        // ファイルパス
        let path_line = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(&self.preview.file_path, Style::default().fg(p.info)),
        ]));
        path_line.render(chunks[0], buf);

        // Before ラベル
        let before_label = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Before:",
                Style::default().fg(p.negative).add_modifier(Modifier::BOLD),
            ),
        ]));
        before_label.render(chunks[1], buf);

        // Before テキスト
        let before_lines =
            Self::build_preview_lines(&self.preview.before_text, half_height as usize, p.fg);
        let before_para = Paragraph::new(before_lines).style(Style::default().bg(p.diff_delete_bg));
        before_para.render(chunks[2], buf);

        // After ラベル
        let after_label = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "After:",
                Style::default().fg(p.positive).add_modifier(Modifier::BOLD),
            ),
        ]));
        after_label.render(chunks[3], buf);

        // After テキスト
        let after_lines =
            Self::build_preview_lines(&self.preview.after_text, half_height as usize, p.fg);
        let after_para = Paragraph::new(after_lines).style(Style::default().bg(p.diff_insert_bg));
        after_para.render(chunks[4], buf);

        // ガイド行
        if chunks.len() > 5 {
            let guide = Paragraph::new(super::confirm_cancel_guide(p, None));
            guide.render(chunks[5], buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::engine::HunkDirection;
    use ratatui::buffer::Buffer;

    #[test]
    fn test_hunk_merge_preview_new_right_to_left() {
        let preview = HunkMergePreview::new(
            "src/a.rs".to_string(),
            HunkDirection::RightToLeft,
            "old text".to_string(),
            "new text".to_string(),
        );
        assert_eq!(preview.file_path, "src/a.rs");
        assert!(preview.direction_label.contains("local"));
        assert_eq!(preview.before_text, "old text");
        assert_eq!(preview.after_text, "new text");
    }

    #[test]
    fn test_hunk_merge_preview_new_left_to_right() {
        let preview = HunkMergePreview::new(
            "src/b.rs".to_string(),
            HunkDirection::LeftToRight,
            "before".to_string(),
            "after".to_string(),
        );
        assert!(preview.direction_label.contains("remote"));
    }

    #[test]
    fn test_build_preview_lines() {
        let lines =
            HunkMergePreviewWidget::build_preview_lines("line1\nline2\nline3", 10, Color::White);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_build_preview_lines_truncates() {
        let text = (0..20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let lines = HunkMergePreviewWidget::build_preview_lines(&text, 5, Color::White);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_hunk_merge_preview_render() {
        let preview = HunkMergePreview::new(
            "src/a.rs".to_string(),
            HunkDirection::RightToLeft,
            "old line".to_string(),
            "new line".to_string(),
        );
        let area = Rect::new(0, 0, 80, 30);
        let mut buf = Buffer::empty(area);
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette = TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = HunkMergePreviewWidget::new(&preview, &palette);
        widget.render(area, &mut buf);

        let content: String = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| {
                        buf.cell((x, y))
                            .map(|c| c.symbol().to_string())
                            .unwrap_or_default()
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(content.contains("Preview"));
        assert!(content.contains("Before"));
        assert!(content.contains("After"));
    }
}
