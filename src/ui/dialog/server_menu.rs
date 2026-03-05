//! サーバ選択メニュー。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::render_dialog_frame;

/// サーバ選択メニューの状態
#[derive(Debug, Clone)]
pub struct ServerMenu {
    /// 利用可能なサーバ名リスト
    pub servers: Vec<String>,
    /// 現在選択中のインデックス
    pub cursor: usize,
    /// 現在接続中のサーバ名
    pub connected: String,
}

impl ServerMenu {
    pub fn new(servers: Vec<String>, connected: String) -> Self {
        let cursor = servers.iter().position(|s| s == &connected).unwrap_or(0);
        Self {
            servers,
            cursor,
            connected,
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
        if self.cursor + 1 < self.servers.len() {
            self.cursor += 1;
        }
    }

    /// 現在選択中のサーバ名を返す
    pub fn selected(&self) -> Option<&str> {
        self.servers.get(self.cursor).map(|s| s.as_str())
    }
}

/// サーバ選択メニューウィジェット
pub struct ServerMenuWidget<'a> {
    menu: &'a ServerMenu,
}

impl<'a> ServerMenuWidget<'a> {
    pub fn new(menu: &'a ServerMenu) -> Self {
        Self { menu }
    }
}

impl<'a> Widget for ServerMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let height = (self.menu.servers.len() as u16) + 4;
        let inner = render_dialog_frame(" Server Select ", Color::Cyan, 40, height, area, buf);

        let lines: Vec<Line> = self
            .menu
            .servers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let is_selected = i == self.menu.cursor;
                let is_connected = name == &self.menu.connected;

                let marker = if is_selected { ">" } else { " " };
                let conn_mark = if is_connected { " (connected)" } else { "" };

                let style = if is_selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else if is_connected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::White)
                };

                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} {}{}", marker, name, conn_mark), style),
                ])
            })
            .collect();

        let list = Paragraph::new(lines);
        list.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_menu_navigation() {
        let mut menu = ServerMenu::new(
            vec![
                "develop".to_string(),
                "staging".to_string(),
                "release".to_string(),
            ],
            "develop".to_string(),
        );

        assert_eq!(menu.cursor, 0);
        assert_eq!(menu.selected(), Some("develop"));

        menu.cursor_down();
        assert_eq!(menu.cursor, 1);
        assert_eq!(menu.selected(), Some("staging"));

        menu.cursor_down();
        assert_eq!(menu.cursor, 2);
        assert_eq!(menu.selected(), Some("release"));

        menu.cursor_down();
        assert_eq!(menu.cursor, 2);

        menu.cursor_up();
        assert_eq!(menu.cursor, 1);

        menu.cursor_up();
        menu.cursor_up();
        assert_eq!(menu.cursor, 0);
    }

    #[test]
    fn test_server_menu_initial_cursor() {
        let menu = ServerMenu::new(
            vec!["develop".to_string(), "staging".to_string()],
            "staging".to_string(),
        );
        assert_eq!(menu.cursor, 1);
    }

    #[test]
    fn test_server_menu_render() {
        let menu = ServerMenu::new(
            vec!["develop".to_string(), "staging".to_string()],
            "develop".to_string(),
        );

        let area = Rect::new(0, 0, 60, 15);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        let widget = ServerMenuWidget::new(&menu);
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

        assert!(content.contains("Server Select"));
    }
}
