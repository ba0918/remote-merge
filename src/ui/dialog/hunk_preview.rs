//! ハンクマージプレビュー。

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::diff::engine::HunkDirection;

use super::render_dialog_frame;

/// ハンクマージプレビューの状態
#[derive(Debug, Clone)]
pub struct HunkMergePreview {
    /// 対象ファイルパス
    pub file_path: String,
    /// マージ方向
    pub direction: HunkDirection,
    /// 適用前テキスト（対象ファイルの変更部分周辺）
    pub before_text: String,
    /// 適用後テキスト
    pub after_text: String,
    /// マージ方向の文字列表示
    pub direction_label: String,
}

impl HunkMergePreview {
    pub fn new(
        file_path: String,
        direction: HunkDirection,
        before_text: String,
        after_text: String,
    ) -> Self {
        let direction_label = match direction {
            HunkDirection::RightToLeft => "remote → local".to_string(),
            HunkDirection::LeftToRight => "local → remote".to_string(),
        };
        Self {
            file_path,
            direction,
            before_text,
            after_text,
            direction_label,
        }
    }
}

/// ハンクマージプレビューウィジェット
pub struct HunkMergePreviewWidget<'a> {
    preview: &'a HunkMergePreview,
    bg: Color,
}

impl<'a> HunkMergePreviewWidget<'a> {
    pub fn new(preview: &'a HunkMergePreview, bg: Color) -> Self {
        Self { preview, bg }
    }

    /// before/after テキストから差分がある行のみを抽出して表示用行を生成
    fn build_preview_lines(text: &str, max_lines: usize) -> Vec<Line<'static>> {
        text.lines()
            .take(max_lines)
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::White),
                ))
            })
            .collect()
    }
}

impl<'a> Widget for HunkMergePreviewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = area.width.min(76);
        let height = area.height.min(24);
        let title = format!(" Hunk Merge Preview ({}) ", self.preview.direction_label);
        let inner = render_dialog_frame(&title, Color::Yellow, width, height, area, buf, self.bg);

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

        // ファイルパス
        let path_line = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(&self.preview.file_path, Style::default().fg(Color::Cyan)),
        ]));
        path_line.render(chunks[0], buf);

        // Before ラベル
        let before_label = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Before:",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));
        before_label.render(chunks[1], buf);

        // Before テキスト
        let before_lines =
            Self::build_preview_lines(&self.preview.before_text, half_height as usize);
        let before_para =
            Paragraph::new(before_lines).style(Style::default().bg(Color::Rgb(30, 0, 0)));
        before_para.render(chunks[2], buf);

        // After ラベル
        let after_label = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "After:",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        after_label.render(chunks[3], buf);

        // After テキスト
        let after_lines = Self::build_preview_lines(&self.preview.after_text, half_height as usize);
        let after_para =
            Paragraph::new(after_lines).style(Style::default().bg(Color::Rgb(0, 30, 0)));
        after_para.render(chunks[4], buf);

        // ガイド行
        if chunks.len() > 5 {
            let guide = Paragraph::new(super::confirm_cancel_guide(None));
            guide.render(chunks[5], buf);
        }
    }
}
