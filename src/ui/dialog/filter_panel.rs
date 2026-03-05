//! フィルターパネル。

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::render_dialog_frame;

/// フィルターパネルの状態
#[derive(Debug, Clone)]
pub struct FilterPanel {
    /// フィルターパターンとその有効/無効状態
    pub patterns: Vec<(String, bool)>,
    /// カーソル位置
    pub cursor: usize,
}

impl FilterPanel {
    pub fn new(patterns: &[String]) -> Self {
        Self {
            patterns: patterns.iter().map(|p| (p.clone(), true)).collect(),
            cursor: 0,
        }
    }

    /// カーソルを上に移動
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// カーソルを下に移動
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.patterns.len() {
            self.cursor += 1;
        }
    }

    /// 現在のパターンの有効/無効をトグル
    pub fn toggle(&mut self) {
        if let Some(item) = self.patterns.get_mut(self.cursor) {
            item.1 = !item.1;
        }
    }

    /// 有効なパターンのみを返す
    pub fn active_patterns(&self) -> Vec<String> {
        self.patterns
            .iter()
            .filter(|(_, enabled)| *enabled)
            .map(|(pattern, _)| pattern.clone())
            .collect()
    }
}

/// フィルターパネルウィジェット
pub struct FilterPanelWidget<'a> {
    panel: &'a FilterPanel,
}

impl<'a> FilterPanelWidget<'a> {
    pub fn new(panel: &'a FilterPanel) -> Self {
        Self { panel }
    }
}

impl<'a> Widget for FilterPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let height = (self.panel.patterns.len() as u16) + 6;
        let inner = render_dialog_frame(" Filters ", Color::Magenta, 50, height, area, buf);

        let constraints: Vec<Constraint> = (0..self.panel.patterns.len())
            .map(|_| Constraint::Length(1))
            .chain(std::iter::once(Constraint::Length(1))) // 空行
            .chain(std::iter::once(Constraint::Length(1))) // ガイド
            .collect();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        for (i, (pattern, enabled)) in self.panel.patterns.iter().enumerate() {
            let is_selected = i == self.panel.cursor;
            let checkbox = if *enabled { "[x]" } else { "[ ]" };

            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else if *enabled {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let line = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} {}", checkbox, pattern), style),
            ]));
            line.render(chunks[i], buf);
        }

        let guide_idx = self.panel.patterns.len() + 1;
        if guide_idx < chunks.len() {
            let guide = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "Space",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(": toggle  "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(": close"),
            ]));
            guide.render(chunks[guide_idx], buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_panel_navigation() {
        let mut panel = FilterPanel::new(&[
            "node_modules".to_string(),
            ".git".to_string(),
            "dist".to_string(),
        ]);

        assert_eq!(panel.cursor, 0);
        assert_eq!(panel.patterns.len(), 3);
        assert!(panel.patterns[0].1);

        panel.cursor_down();
        assert_eq!(panel.cursor, 1);

        panel.cursor_down();
        assert_eq!(panel.cursor, 2);

        panel.cursor_down();
        assert_eq!(panel.cursor, 2);

        panel.cursor_up();
        assert_eq!(panel.cursor, 1);
    }

    #[test]
    fn test_filter_panel_toggle() {
        let mut panel = FilterPanel::new(&["node_modules".to_string(), ".git".to_string()]);

        assert_eq!(panel.active_patterns().len(), 2);

        panel.toggle();
        assert!(!panel.patterns[0].1);
        assert_eq!(panel.active_patterns(), vec![".git"]);

        panel.toggle();
        assert!(panel.patterns[0].1);
        assert_eq!(panel.active_patterns().len(), 2);
    }

    #[test]
    fn test_filter_panel_active_patterns() {
        let mut panel = FilterPanel::new(&["a".to_string(), "b".to_string(), "c".to_string()]);

        panel.cursor = 1;
        panel.toggle();

        let active = panel.active_patterns();
        assert_eq!(active, vec!["a", "c"]);
    }

    #[test]
    fn test_filter_panel_render() {
        let panel = FilterPanel::new(&["node_modules".to_string(), ".git".to_string()]);

        let area = Rect::new(0, 0, 60, 15);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        let widget = FilterPanelWidget::new(&panel);
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

        assert!(content.contains("Filters"));
    }
}
