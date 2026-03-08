//! 3way サマリーパネルの描画ウィジェット。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::three_way_summary::ThreeWaySummaryPanel;

/// 3way サマリーパネルの描画ウィジェット
pub struct ThreeWaySummaryWidget<'a> {
    panel: &'a ThreeWaySummaryPanel,
    bg: Color,
}

impl<'a> ThreeWaySummaryWidget<'a> {
    pub fn new(panel: &'a ThreeWaySummaryPanel, bg: Color) -> Self {
        Self { panel, bg }
    }
}

/// UTF-8 安全な文字列トランケート
fn truncate_chars(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    }
}

impl Widget for ThreeWaySummaryWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // ダイアログサイズ: 幅は画面の 80% (最大 80)、高さは min(行数+4, 画面の 70%)
        let width = (area.width * 80 / 100).clamp(30, 80);
        let content_height = self.panel.lines.len().min(20) as u16;
        let height = (content_height + 4).min(area.height * 70 / 100).max(6);

        // ファイル名をタイトルに
        let file_name = self
            .panel
            .file_path
            .rsplit('/')
            .next()
            .unwrap_or(&self.panel.file_path);
        let title = format!(" 3way Summary: {} ", file_name);

        let inner =
            super::render_dialog_frame(&title, Color::Yellow, width, height, area, buf, self.bg);

        if inner.height < 2 {
            return;
        }

        // フッター用に 1行確保
        let list_height = (inner.height.saturating_sub(1)) as usize;

        if self.panel.lines.is_empty() {
            // 空の場合のメッセージ
            let msg_line = Line::from(vec![
                Span::raw("  "),
                Span::styled("No differences found", Style::default().fg(Color::DarkGray)),
            ]);
            buf.set_line(inner.x, inner.y, &msg_line, inner.width);
        } else {
            // ラベルのカラム幅（固定20文字で値をトランケート）
            let col_width: usize = 20;

            // 各行を描画
            let visible_start = self.panel.scroll;
            let visible_end = (visible_start + list_height).min(self.panel.lines.len());

            for (vi, idx) in (visible_start..visible_end).enumerate() {
                let line = &self.panel.lines[idx];
                let y = inner.y + vi as u16;
                let is_cursor = idx == self.panel.cursor;

                // カーソル行の背景色
                let line_bg = if is_cursor { Color::DarkGray } else { self.bg };

                // 行全体を背景色で塗る
                let bg_style = Style::default().bg(line_bg);
                for x in inner.x..inner.x + inner.width {
                    buf[(x, y)].set_style(bg_style);
                }

                // 行番号
                let line_num = match line.display_line_number {
                    Some(n) => format!("L{:<4}", n),
                    None => "L+   ".to_string(),
                };

                let mut spans = vec![Span::styled(
                    format!(" {}", line_num),
                    Style::default().fg(Color::DarkGray).bg(line_bg),
                )];

                // 3つのカラム: left, right, ref
                let labels = [
                    &self.panel.left_label,
                    &self.panel.right_label,
                    &self.panel.ref_label,
                ];
                let contents = [&line.left_content, &line.right_content, &line.ref_content];
                let colors = [Color::Green, Color::Blue, Color::Magenta];

                for i in 0..3 {
                    let val = match contents[i] {
                        Some(s) => {
                            let trimmed = s.trim();
                            truncate_chars(trimmed, col_width)
                        }
                        None => "[ \u{2014} ]".to_string(),
                    };
                    spans.push(Span::styled(
                        format!(" {}=", labels[i]),
                        Style::default().fg(colors[i]).bg(line_bg),
                    ));
                    spans.push(Span::styled(
                        format!("\"{}\"", val),
                        Style::default().fg(Color::White).bg(line_bg),
                    ));
                }

                let rendered_line = Line::from(spans);
                buf.set_line(inner.x, y, &rendered_line, inner.width);
            }
        }

        // フッター
        let footer_y = inner.y + inner.height.saturating_sub(1);
        let footer = Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "[Enter]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Jump to line  "),
            Span::styled(
                "[W/Esc]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Close"),
        ]);
        buf.set_line(inner.x, footer_y, &footer, inner.width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_chars_short() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_chars_exact() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_chars_long() {
        let result = truncate_chars("hello world", 5);
        assert_eq!(result, "hell\u{2026}");
    }

    #[test]
    fn test_truncate_chars_multibyte() {
        let result = truncate_chars("\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}", 3);
        // 2文字 + 省略記号
        assert_eq!(result, "\u{3042}\u{3044}\u{2026}");
    }
}
