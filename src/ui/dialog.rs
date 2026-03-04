//! モーダルダイアログ（確認ダイアログ + サーバ選択メニュー）の描画。

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::merge::executor::MergeDirection;

/// マージ確認ダイアログの状態
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    /// マージ対象のファイルパス
    pub file_path: String,
    /// マージの方向
    pub direction: MergeDirection,
    /// ソース名（例: "local"）
    pub source_name: String,
    /// ターゲット名（例: "develop"）
    pub target_name: String,
}

impl ConfirmDialog {
    pub fn new(
        file_path: String,
        direction: MergeDirection,
        source_name: String,
        target_name: String,
    ) -> Self {
        Self {
            file_path,
            direction,
            source_name,
            target_name,
        }
    }

    /// ダイアログのメッセージを生成
    pub fn message(&self) -> String {
        match self.direction {
            MergeDirection::LeftMerge => {
                format!(
                    "{} を {} → {} にマージしますか？",
                    self.file_path, self.source_name, self.target_name
                )
            }
            MergeDirection::RightMerge => {
                format!(
                    "{} を {} → {} にマージしますか？",
                    self.file_path, self.target_name, self.source_name
                )
            }
        }
    }
}

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
        let cursor = servers
            .iter()
            .position(|s| s == &connected)
            .unwrap_or(0);
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

/// アプリのダイアログ状態
#[derive(Debug, Clone, Default)]
pub enum DialogState {
    /// ダイアログなし
    #[default]
    None,
    /// マージ確認ダイアログ
    Confirm(ConfirmDialog),
    /// サーバ選択メニュー
    ServerSelect(ServerMenu),
}

/// 中央にモーダルエリアを計算する
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// 確認ダイアログウィジェット
pub struct ConfirmDialogWidget<'a> {
    dialog: &'a ConfirmDialog,
}

impl<'a> ConfirmDialogWidget<'a> {
    pub fn new(dialog: &'a ConfirmDialog) -> Self {
        Self { dialog }
    }
}

impl<'a> Widget for ConfirmDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog_area = centered_rect(60, 7, area);

        // 背景をクリア
        Clear.render(dialog_area, buf);

        let block = Block::default()
            .title(" Merge Confirmation ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

        let inner = block.inner(dialog_area);
        block.render(dialog_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // 空行
                Constraint::Length(1), // メッセージ
                Constraint::Length(1), // 空行
                Constraint::Length(1), // Y/n ガイド
            ])
            .split(inner);

        let msg = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                self.dialog.message(),
                Style::default().fg(Color::White),
            ),
        ]));
        msg.render(chunks[1], buf);

        let guide = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("[Y]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" 実行  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" キャンセル"),
        ]));
        guide.render(chunks[3], buf);
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
        let height = (self.menu.servers.len() as u16) + 4; // borders + title + padding
        let dialog_area = centered_rect(40, height, area);

        // 背景をクリア
        Clear.render(dialog_area, buf);

        let block = Block::default()
            .title(" Server Select ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

        let inner = block.inner(dialog_area);
        block.render(dialog_area, buf);

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
    fn test_confirm_dialog_message_left_merge() {
        let dialog = ConfirmDialog::new(
            "src/config.ts".to_string(),
            MergeDirection::LeftMerge,
            "local".to_string(),
            "develop".to_string(),
        );
        assert_eq!(
            dialog.message(),
            "src/config.ts を local → develop にマージしますか？"
        );
    }

    #[test]
    fn test_confirm_dialog_message_right_merge() {
        let dialog = ConfirmDialog::new(
            "src/config.ts".to_string(),
            MergeDirection::RightMerge,
            "local".to_string(),
            "develop".to_string(),
        );
        assert_eq!(
            dialog.message(),
            "src/config.ts を develop → local にマージしますか？"
        );
    }

    #[test]
    fn test_server_menu_navigation() {
        let mut menu = ServerMenu::new(
            vec!["develop".to_string(), "staging".to_string(), "release".to_string()],
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

        // 下限
        menu.cursor_down();
        assert_eq!(menu.cursor, 2);

        menu.cursor_up();
        assert_eq!(menu.cursor, 1);

        // 上限
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
        // connected サーバがカーソル初期位置
        assert_eq!(menu.cursor, 1);
    }

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 40);
        let rect = centered_rect(60, 10, area);

        assert_eq!(rect.x, 20);
        assert_eq!(rect.y, 15);
        assert_eq!(rect.width, 60);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn test_centered_rect_smaller_area() {
        let area = Rect::new(0, 0, 30, 5);
        let rect = centered_rect(60, 10, area);

        // 面積がダイアログより小さい場合
        assert_eq!(rect.width, 30);
        assert_eq!(rect.height, 5);
    }

    #[test]
    fn test_confirm_dialog_render() {
        let dialog = ConfirmDialog::new(
            "test.txt".to_string(),
            MergeDirection::LeftMerge,
            "local".to_string(),
            "develop".to_string(),
        );

        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        let widget = ConfirmDialogWidget::new(&dialog);
        widget.render(area, &mut buf);

        // パニックしなければOK
        let content: String = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf.cell((x, y)).map(|c| c.symbol().to_string()).unwrap_or_default())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(content.contains("Merge Confirmation"));
    }

    #[test]
    fn test_server_menu_render() {
        let menu = ServerMenu::new(
            vec!["develop".to_string(), "staging".to_string()],
            "develop".to_string(),
        );

        let area = Rect::new(0, 0, 60, 15);
        let mut buf = Buffer::empty(area);
        let widget = ServerMenuWidget::new(&menu);
        widget.render(area, &mut buf);

        let content: String = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf.cell((x, y)).map(|c| c.symbol().to_string()).unwrap_or_default())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(content.contains("Server Select"));
    }

    #[test]
    fn test_dialog_state_default() {
        let state = DialogState::default();
        assert!(matches!(state, DialogState::None));
    }
}
