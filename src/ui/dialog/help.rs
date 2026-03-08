//! ヘルプオーバーレイ。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::render_dialog_frame;

/// ヘルプオーバーレイのセクション
#[derive(Debug, Clone)]
pub struct HelpSection {
    pub title: String,
    pub bindings: Vec<(String, String)>, // (キー, 説明)
}

/// ヘルプオーバーレイの状態
#[derive(Debug, Clone)]
pub struct HelpOverlay {
    pub sections: Vec<HelpSection>,
    /// スクロールオフセット（行単位）
    pub scroll: usize,
}

impl Default for HelpOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpOverlay {
    /// 全セクション合計の行数を計算する
    pub fn total_lines(&self) -> usize {
        self.sections
            .iter()
            .map(|s| s.bindings.len() + 2) // タイトル行 + 空行 + bindings
            .sum()
    }

    /// 下にスクロール
    pub fn scroll_down(&mut self) {
        let max = self.total_lines().saturating_sub(1);
        if self.scroll < max {
            self.scroll += 1;
        }
    }

    /// 上にスクロール
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// ページ下スクロール
    pub fn page_down(&mut self, page_size: usize) {
        let max = self.total_lines().saturating_sub(1);
        self.scroll = (self.scroll + page_size).min(max);
    }

    /// ページ上スクロール
    pub fn page_up(&mut self, page_size: usize) {
        self.scroll = self.scroll.saturating_sub(page_size);
    }

    pub fn new() -> Self {
        Self {
            scroll: 0,
            sections: vec![
                HelpSection {
                    title: "File Tree".to_string(),
                    bindings: vec![
                        ("j/↓".to_string(), "Move cursor down".to_string()),
                        ("k/↑".to_string(), "Move cursor up".to_string()),
                        ("Enter/l/→".to_string(), "Expand / Select file".to_string()),
                        ("h/←".to_string(), "Collapse".to_string()),
                        (
                            "L (Shift)".to_string(),
                            "Merge remote → local (dir supported)".to_string(),
                        ),
                        (
                            "R (Shift)".to_string(),
                            "Merge local → remote (dir supported)".to_string(),
                        ),
                        (
                            "F (Shift)".to_string(),
                            "Show changed files only (full scan)".to_string(),
                        ),
                        ("c".to_string(), "Copy diff to clipboard".to_string()),
                        ("r".to_string(), "Refresh dir / Reconnect SSH".to_string()),
                        ("f".to_string(), "Filter panel".to_string()),
                        ("s".to_string(), "Server select".to_string()),
                        ("W (Shift)".to_string(), "3way summary panel".to_string()),
                        (
                            "X (Shift)".to_string(),
                            "Swap right ↔ ref server".to_string(),
                        ),
                        ("/".to_string(), "Search files".to_string()),
                        ("n".to_string(), "Next search match".to_string()),
                        ("N (Shift)".to_string(), "Previous search match".to_string()),
                        (
                            "E (Shift)".to_string(),
                            "Export report (Markdown)".to_string(),
                        ),
                    ],
                },
                HelpSection {
                    title: "Diff View".to_string(),
                    bindings: vec![
                        ("j/k/↑/↓".to_string(), "Scroll one line".to_string()),
                        ("n".to_string(), "Next hunk / search match".to_string()),
                        ("N".to_string(), "Prev hunk / search match".to_string()),
                        ("/".to_string(), "Search in diff".to_string()),
                        ("PageDown".to_string(), "Page down".to_string()),
                        ("PageUp".to_string(), "Page up".to_string()),
                        ("Home".to_string(), "Go to top".to_string()),
                        ("End".to_string(), "Go to bottom".to_string()),
                        ("→/l".to_string(), "Hunk: apply remote → local".to_string()),
                        ("←/h".to_string(), "Hunk: apply local → remote".to_string()),
                        ("w".to_string(), "Write changes to file".to_string()),
                        ("u".to_string(), "Undo last change".to_string()),
                        ("U".to_string(), "Undo all changes".to_string()),
                        ("d".to_string(), "Toggle Unified / Side-by-Side".to_string()),
                        ("c".to_string(), "Copy diff to clipboard".to_string()),
                        ("r".to_string(), "Reconnect SSH".to_string()),
                        ("W (Shift)".to_string(), "3way summary panel".to_string()),
                    ],
                },
                HelpSection {
                    title: "Global".to_string(),
                    bindings: vec![
                        ("Tab".to_string(), "Toggle focus".to_string()),
                        ("T".to_string(), "Cycle theme".to_string()),
                        ("S".to_string(), "Syntax highlight ON/OFF".to_string()),
                        ("?".to_string(), "Toggle help".to_string()),
                        ("q".to_string(), "Quit".to_string()),
                    ],
                },
            ],
        }
    }
}

/// ヘルプオーバーレイウィジェット
pub struct HelpOverlayWidget<'a> {
    help: &'a HelpOverlay,
    bg: Color,
}

impl<'a> HelpOverlayWidget<'a> {
    pub fn new(help: &'a HelpOverlay, bg: Color) -> Self {
        Self { help, bg }
    }
}

impl<'a> Widget for HelpOverlayWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let total_lines = self.help.total_lines();

        let width = area.width.min(60);
        let height = ((total_lines as u16) + 4).min(area.height);

        // スクロール可能ならタイトルにインジケータ追加
        let title = if total_lines + 2 > height as usize {
            " Help (j/k scroll, ? close) "
        } else {
            " Help (? to close) "
        };

        let inner = render_dialog_frame(title, Color::Cyan, width, height, area, buf, self.bg);

        let mut lines: Vec<Line> = Vec::new();

        for section in &self.help.sections {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("── {} ──", section.title),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            for (key, desc) in &section.bindings {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<16}", key), Style::default().fg(Color::Cyan)),
                    Span::styled(desc.clone(), Style::default().fg(Color::White)),
                ]));
            }

            lines.push(Line::from(""));
        }

        // スクロールオフセットを適用
        let visible_lines: Vec<Line> = lines.into_iter().skip(self.help.scroll).collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    #[test]
    fn test_help_overlay_new() {
        let help = HelpOverlay::new();
        assert_eq!(help.sections.len(), 3);
        assert_eq!(help.sections[0].title, "File Tree");
        assert_eq!(help.sections[1].title, "Diff View");
        assert_eq!(help.sections[2].title, "Global");
    }

    #[test]
    fn test_help_overlay_default() {
        let help = HelpOverlay::default();
        assert_eq!(help.sections.len(), 3);
    }

    #[test]
    fn test_help_sections_have_bindings() {
        let help = HelpOverlay::new();
        for section in &help.sections {
            assert!(
                !section.bindings.is_empty(),
                "{} should have bindings",
                section.title
            );
        }
    }

    #[test]
    fn test_help_overlay_render() {
        let help = HelpOverlay::new();
        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        let widget = HelpOverlayWidget::new(&help, Color::Rgb(0x2b, 0x30, 0x3b));
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
        assert!(content.contains("Help"));
        assert!(content.contains("File Tree"));
        assert!(content.contains("Diff View"));
    }

    #[test]
    fn test_help_scroll_down_and_up() {
        let mut help = HelpOverlay::new();
        assert_eq!(help.scroll, 0);
        help.scroll_down();
        assert_eq!(help.scroll, 1);
        help.scroll_down();
        assert_eq!(help.scroll, 2);
        help.scroll_up();
        assert_eq!(help.scroll, 1);
        help.scroll_up();
        assert_eq!(help.scroll, 0);
        // 0 より下にはいかない
        help.scroll_up();
        assert_eq!(help.scroll, 0);
    }

    #[test]
    fn test_help_scroll_max() {
        let mut help = HelpOverlay::new();
        let max = help.total_lines().saturating_sub(1);
        // max を超えてスクロールしない
        help.scroll = max;
        help.scroll_down();
        assert_eq!(help.scroll, max);
    }

    #[test]
    fn test_help_page_scroll() {
        let mut help = HelpOverlay::new();
        help.page_down(10);
        assert_eq!(help.scroll, 10);
        help.page_up(5);
        assert_eq!(help.scroll, 5);
        help.page_up(100);
        assert_eq!(help.scroll, 0);
    }

    #[test]
    fn test_help_total_lines() {
        let help = HelpOverlay::new();
        // 各セクション: bindings.len() + 2 (タイトル + 空行)
        let expected: usize = help.sections.iter().map(|s| s.bindings.len() + 2).sum();
        assert_eq!(help.total_lines(), expected);
        assert!(help.total_lines() > 30); // キーバインドは十分多い
    }

    #[test]
    fn test_help_contains_quit_binding() {
        let help = HelpOverlay::new();
        let global = &help.sections[2];
        assert!(
            global.bindings.iter().any(|(k, _)| k == "q"),
            "Should have quit binding"
        );
    }
}
