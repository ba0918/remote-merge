//! マージ確認ダイアログ。

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::merge::executor::MergeDirection;

use super::render_dialog_frame;

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
            MergeDirection::LocalToRemote => {
                format!(
                    "{} を {} → {} にマージしますか？",
                    self.file_path, self.source_name, self.target_name
                )
            }
            MergeDirection::RemoteToLocal => {
                format!(
                    "{} を {} → {} にマージしますか？",
                    self.file_path, self.source_name, self.target_name
                )
            }
        }
    }
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
        let inner = render_dialog_frame(" Merge Confirmation ", Color::Yellow, 60, 7, area, buf);

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
            Span::styled(self.dialog.message(), Style::default().fg(Color::White)),
        ]));
        msg.render(chunks[1], buf);

        let guide = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "[Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" 実行  "),
            Span::styled(
                "[n/Esc]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" キャンセル"),
        ]));
        guide.render(chunks[3], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirm_dialog_message_left_merge() {
        let dialog = ConfirmDialog::new(
            "src/config.ts".to_string(),
            MergeDirection::LocalToRemote,
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
            MergeDirection::RemoteToLocal,
            "develop".to_string(),
            "local".to_string(),
        );
        assert_eq!(
            dialog.message(),
            "src/config.ts を develop → local にマージしますか？"
        );
    }

    #[test]
    fn test_confirm_dialog_both_directions_use_source_arrow_target() {
        let left = ConfirmDialog::new(
            "app.js".to_string(),
            MergeDirection::LocalToRemote,
            "local".to_string(),
            "staging".to_string(),
        );
        assert!(left.message().contains("local → staging"));

        let right = ConfirmDialog::new(
            "app.js".to_string(),
            MergeDirection::RemoteToLocal,
            "staging".to_string(),
            "local".to_string(),
        );
        assert!(right.message().contains("staging → local"));
    }

    #[test]
    fn test_confirm_dialog_render() {
        let dialog = ConfirmDialog::new(
            "test.txt".to_string(),
            MergeDirection::LocalToRemote,
            "local".to_string(),
            "develop".to_string(),
        );

        let area = Rect::new(0, 0, 80, 20);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        let widget = ConfirmDialogWidget::new(&dialog);
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

        assert!(content.contains("Merge Confirmation"));
    }
}
