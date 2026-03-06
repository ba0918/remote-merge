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
}

impl Default for HelpOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpOverlay {
    pub fn new() -> Self {
        Self {
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
                        ("r".to_string(), "Refresh / Clear cache".to_string()),
                        ("f".to_string(), "Filter panel".to_string()),
                        ("s".to_string(), "Server select".to_string()),
                    ],
                },
                HelpSection {
                    title: "Diff View".to_string(),
                    bindings: vec![
                        ("j/k/↑/↓".to_string(), "Scroll one line".to_string()),
                        ("n".to_string(), "Jump to next hunk".to_string()),
                        ("N".to_string(), "Jump to prev hunk".to_string()),
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
                        ("c".to_string(), "Reconnect SSH".to_string()),
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
        let total_lines: usize = self
            .help
            .sections
            .iter()
            .map(|s| s.bindings.len() + 2)
            .sum();

        let width = area.width.min(60);
        let height = ((total_lines as u16) + 4).min(area.height);
        let inner = render_dialog_frame(
            " Help (? to close) ",
            Color::Cyan,
            width,
            height,
            area,
            buf,
            self.bg,
        );

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

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}
