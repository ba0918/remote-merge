//! ペアサーバ選択メニュー（LEFT/RIGHT 2列選択）。
//!
//! 3way diff でサーバペアを自由に切り替えるためのダイアログ。
//! LEFT 列と RIGHT 列を Tab で切り替え、各列で独立にカーソル移動できる。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::render_dialog_frame;

/// アクティブ列
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Left,
    Right,
}

/// ペアサーバ選択メニューの状態
#[derive(Debug, Clone)]
pub struct PairServerMenu {
    /// 利用可能なサーバ名リスト（"local" を含む）
    pub servers: Vec<String>,
    /// LEFT 列のカーソル位置
    pub left_cursor: usize,
    /// RIGHT 列のカーソル位置
    pub right_cursor: usize,
    /// アクティブ列
    pub active_column: Column,
}

impl PairServerMenu {
    /// 新しい PairServerMenu を構築する。
    /// カーソルは現在の left/right サーバに初期配置。
    pub fn new(servers: Vec<String>, current_left: &str, current_right: &str) -> Self {
        let left_cursor = servers.iter().position(|s| s == current_left).unwrap_or(0);
        let right_cursor = servers
            .iter()
            .position(|s| s == current_right)
            .unwrap_or(if servers.len() > 1 { 1 } else { 0 });
        Self {
            servers,
            left_cursor,
            right_cursor,
            active_column: Column::Left,
        }
    }

    /// アクティブ列を切り替える
    pub fn toggle_column(&mut self) {
        self.active_column = match self.active_column {
            Column::Left => Column::Right,
            Column::Right => Column::Left,
        };
    }

    /// アクティブ列のカーソルを上に移動
    pub fn cursor_up(&mut self) {
        let cursor = self.active_cursor_mut();
        if *cursor > 0 {
            *cursor -= 1;
        }
    }

    /// アクティブ列のカーソルを下に移動
    pub fn cursor_down(&mut self) {
        let max = self.servers.len().saturating_sub(1);
        let cursor = self.active_cursor_mut();
        if *cursor < max {
            *cursor += 1;
        }
    }

    /// LEFT 列の選択サーバ名
    pub fn selected_left(&self) -> Option<&str> {
        self.servers.get(self.left_cursor).map(|s| s.as_str())
    }

    /// RIGHT 列の選択サーバ名
    pub fn selected_right(&self) -> Option<&str> {
        self.servers.get(self.right_cursor).map(|s| s.as_str())
    }

    /// 左右が同じサーバを選択しているか
    pub fn is_same_pair(&self) -> bool {
        self.left_cursor == self.right_cursor
    }

    /// アクティブ列のカーソルへの可変参照
    fn active_cursor_mut(&mut self) -> &mut usize {
        match self.active_column {
            Column::Left => &mut self.left_cursor,
            Column::Right => &mut self.right_cursor,
        }
    }
}

/// ペアサーバ選択メニューウィジェット
pub struct PairServerMenuWidget<'a> {
    menu: &'a PairServerMenu,
    palette: &'a crate::theme::palette::TuiPalette,
}

impl<'a> PairServerMenuWidget<'a> {
    pub fn new(menu: &'a PairServerMenu, palette: &'a crate::theme::palette::TuiPalette) -> Self {
        Self { menu, palette }
    }
}

impl<'a> Widget for PairServerMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let server_count = self.menu.servers.len() as u16;
        // ヘッダー(2行) + サーバリスト + フッター(2行) + ペア表示(1行)
        let height = server_count + 6;
        let width = 60u16;
        let p = self.palette;
        let inner = render_dialog_frame(
            " Server Pair Select ",
            p.info,
            width,
            height,
            area,
            buf,
            p.bg,
        );

        let mut lines: Vec<Line> = Vec::new();

        // ヘッダー: LEFT / RIGHT 列タイトル
        let left_style = if self.menu.active_column == Column::Left {
            Style::default()
                .fg(p.info)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(p.muted)
        };
        let right_style = if self.menu.active_column == Column::Right {
            Style::default()
                .fg(p.info)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(p.muted)
        };

        let col_width = (inner.width as usize).saturating_sub(4) / 2;

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{:<width$}", "LEFT", width = col_width), left_style),
            Span::styled(
                format!("{:<width$}", "RIGHT", width = col_width),
                right_style,
            ),
        ]));
        lines.push(Line::from(""));

        // サーバリスト（左右2列表示）
        for (i, name) in self.menu.servers.iter().enumerate() {
            let left_selected = i == self.menu.left_cursor;
            let right_selected = i == self.menu.right_cursor;

            let left_marker = if left_selected { ">" } else { " " };
            let right_marker = if right_selected { ">" } else { " " };

            let left_cell_style = if left_selected && self.menu.active_column == Column::Left {
                Style::default()
                    .fg(p.fg)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else if left_selected {
                Style::default().fg(p.positive).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(p.fg)
            };

            let right_cell_style = if right_selected && self.menu.active_column == Column::Right {
                Style::default()
                    .fg(p.fg)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else if right_selected {
                Style::default().fg(p.positive).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(p.fg)
            };

            let left_text = format!(
                "{} {:<width$}",
                left_marker,
                name,
                width = col_width.saturating_sub(2)
            );
            let right_text = format!(
                "{} {:<width$}",
                right_marker,
                name,
                width = col_width.saturating_sub(2)
            );

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(left_text, left_cell_style),
                Span::styled(right_text, right_cell_style),
            ]));
        }

        // 空行
        lines.push(Line::from(""));

        // 現在のペア表示
        let left_name = self.menu.selected_left().unwrap_or("?");
        let right_name = self.menu.selected_right().unwrap_or("?");
        let pair_style = if self.menu.is_same_pair() {
            Style::default().fg(p.negative)
        } else {
            Style::default().fg(p.positive)
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{} <-> {}", left_name, right_name), pair_style),
        ]));

        // フッター
        let warn = if self.menu.is_same_pair() {
            Span::styled(
                "  (same server)",
                Style::default().fg(p.negative).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        };

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "[Tab]",
                Style::default().fg(p.info).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Column  "),
            Span::styled(
                "[Enter]",
                Style::default().fg(p.positive).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" OK  "),
            Span::styled(
                "[Esc]",
                Style::default().fg(p.negative).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Cancel"),
            warn,
        ]));

        let list = Paragraph::new(lines);
        list.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_servers() -> Vec<String> {
        vec![
            "local".to_string(),
            "develop".to_string(),
            "staging".to_string(),
        ]
    }

    #[test]
    fn test_initial_cursor_positions() {
        let menu = PairServerMenu::new(make_servers(), "local", "develop");
        assert_eq!(menu.left_cursor, 0);
        assert_eq!(menu.right_cursor, 1);
        assert_eq!(menu.active_column, Column::Left);
    }

    #[test]
    fn test_toggle_column() {
        let mut menu = PairServerMenu::new(make_servers(), "local", "develop");
        assert_eq!(menu.active_column, Column::Left);
        menu.toggle_column();
        assert_eq!(menu.active_column, Column::Right);
        menu.toggle_column();
        assert_eq!(menu.active_column, Column::Left);
    }

    #[test]
    fn test_cursor_movement_left_column() {
        let mut menu = PairServerMenu::new(make_servers(), "local", "develop");
        assert_eq!(menu.left_cursor, 0);

        menu.cursor_down();
        assert_eq!(menu.left_cursor, 1);

        menu.cursor_down();
        assert_eq!(menu.left_cursor, 2);

        // 境界チェック: 下限
        menu.cursor_down();
        assert_eq!(menu.left_cursor, 2);

        menu.cursor_up();
        assert_eq!(menu.left_cursor, 1);

        menu.cursor_up();
        menu.cursor_up();
        // 境界チェック: 上限
        assert_eq!(menu.left_cursor, 0);
    }

    #[test]
    fn test_cursor_movement_right_column() {
        let mut menu = PairServerMenu::new(make_servers(), "local", "develop");
        menu.toggle_column(); // RIGHT 列に切り替え
        assert_eq!(menu.right_cursor, 1);

        menu.cursor_down();
        assert_eq!(menu.right_cursor, 2);

        menu.cursor_up();
        assert_eq!(menu.right_cursor, 1);
    }

    #[test]
    fn test_selected_left_right() {
        let menu = PairServerMenu::new(make_servers(), "local", "staging");
        assert_eq!(menu.selected_left(), Some("local"));
        assert_eq!(menu.selected_right(), Some("staging"));
    }

    #[test]
    fn test_is_same_pair() {
        let mut menu = PairServerMenu::new(make_servers(), "local", "local");
        // left_cursor=0, right_cursor=0
        assert!(menu.is_same_pair());

        menu.toggle_column();
        menu.cursor_down();
        assert!(!menu.is_same_pair());
    }

    #[test]
    fn test_render_does_not_panic() {
        let menu = PairServerMenu::new(make_servers(), "local", "develop");
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette =
            crate::theme::palette::TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = PairServerMenuWidget::new(&menu, &palette);
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

        assert!(content.contains("Server Pair Select"));
        assert!(content.contains("LEFT"));
        assert!(content.contains("RIGHT"));
    }
}
