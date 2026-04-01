//! ヘルプオーバーレイ（Widget のみ。データ型は app/dialog_types.rs）。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::dialog_types::HelpOverlay;
use crate::theme::palette::TuiPalette;

use super::render_dialog_frame;

/// ヘルプオーバーレイウィジェット
pub struct HelpOverlayWidget<'a> {
    help: &'a HelpOverlay,
    palette: &'a TuiPalette,
}

impl<'a> HelpOverlayWidget<'a> {
    pub fn new(help: &'a HelpOverlay, palette: &'a TuiPalette) -> Self {
        Self { help, palette }
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

        let inner = render_dialog_frame(
            title,
            self.palette.info,
            width,
            height,
            area,
            buf,
            self.palette.bg,
        );

        let mut lines: Vec<Line> = Vec::new();

        for section in &self.help.sections {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("── {} ──", section.title),
                    Style::default()
                        .fg(self.palette.dialog_accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            for (key, desc) in &section.bindings {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<16}", key),
                        Style::default().fg(self.palette.info),
                    ),
                    Span::styled(desc.clone(), Style::default().fg(self.palette.fg)),
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
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette = TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = HelpOverlayWidget::new(&help, &palette);
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
