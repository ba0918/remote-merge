//! マージ確認ダイアログ。

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::merge::executor::MergeDirection;
use crate::theme::palette::TuiPalette;

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
    /// リモート間マージかどうか（追加の警告表示に使用）
    pub is_remote_to_remote: bool,
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
            is_remote_to_remote: false,
        }
    }

    /// リモート間マージフラグを設定する
    pub fn with_remote_to_remote(mut self, is_r2r: bool) -> Self {
        self.is_remote_to_remote = is_r2r;
        self
    }

    /// ダイアログのメッセージ行を生成
    pub fn message_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Merge {} from", self.file_path),
            format!("{} → {}?", self.source_name, self.target_name),
        ];
        if self.is_remote_to_remote {
            lines.push(String::new());
            lines.push("⚠ Remote-to-remote merge".to_string());
        }
        lines
    }

    /// ダイアログのメッセージを生成（テスト用の後方互換）
    pub fn message(&self) -> String {
        self.message_lines().join("\n")
    }
}

/// 確認ダイアログウィジェット
pub struct ConfirmDialogWidget<'a> {
    dialog: &'a ConfirmDialog,
    palette: &'a TuiPalette,
}

impl<'a> ConfirmDialogWidget<'a> {
    pub fn new(dialog: &'a ConfirmDialog, palette: &'a TuiPalette) -> Self {
        Self { dialog, palette }
    }
}

impl<'a> Widget for ConfirmDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = self.dialog.message_lines();
        let msg_height = lines.len() as u16;
        // 上余白(1) + メッセージ行 + 下余白(1) + ガイド(1) + 枠線(2)
        let dialog_height = msg_height + 5;

        let inner = render_dialog_frame(
            " Merge Confirmation ",
            self.palette.dialog_accent,
            60,
            dialog_height,
            area,
            buf,
            self.palette.bg,
        );

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),          // 上余白
                Constraint::Length(msg_height), // メッセージ
                Constraint::Length(1),          // 下余白
                Constraint::Length(1),          // Y/n ガイド
            ])
            .split(inner);

        let msg_lines: Vec<Line> = lines
            .iter()
            .map(|l| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(l.as_str(), Style::default().fg(Color::White)),
                ])
            })
            .collect();
        let msg = Paragraph::new(msg_lines);
        msg.render(chunks[1], buf);

        let guide = Paragraph::new(super::confirm_cancel_guide(None));
        guide.render(chunks[3], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirm_dialog_message_lines_left_merge() {
        let dialog = ConfirmDialog::new(
            "src/config.ts".to_string(),
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
        );
        let lines = dialog.message_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Merge src/config.ts from");
        assert_eq!(lines[1], "local → develop?");
    }

    #[test]
    fn test_confirm_dialog_message_lines_right_merge() {
        let dialog = ConfirmDialog::new(
            "src/config.ts".to_string(),
            MergeDirection::RightToLeft,
            "develop".to_string(),
            "local".to_string(),
        );
        let lines = dialog.message_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Merge src/config.ts from");
        assert_eq!(lines[1], "develop → local?");
    }

    #[test]
    fn test_confirm_dialog_message_lines_remote_to_remote() {
        let dialog = ConfirmDialog::new(
            "app.js".to_string(),
            MergeDirection::LeftToRight,
            "staging".to_string(),
            "production".to_string(),
        )
        .with_remote_to_remote(true);
        let lines = dialog.message_lines();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "Merge app.js from");
        assert_eq!(lines[1], "staging → production?");
        assert_eq!(lines[2], "");
        assert_eq!(lines[3], "⚠ Remote-to-remote merge");
    }

    #[test]
    fn test_confirm_dialog_both_directions_use_source_arrow_target() {
        let left = ConfirmDialog::new(
            "app.js".to_string(),
            MergeDirection::LeftToRight,
            "local".to_string(),
            "staging".to_string(),
        );
        assert!(left.message().contains("local → staging"));

        let right = ConfirmDialog::new(
            "app.js".to_string(),
            MergeDirection::RightToLeft,
            "staging".to_string(),
            "local".to_string(),
        );
        assert!(right.message().contains("staging → local"));
    }

    #[test]
    fn test_confirm_dialog_render() {
        let dialog = ConfirmDialog::new(
            "test.txt".to_string(),
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
        );

        let area = Rect::new(0, 0, 80, 20);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette = TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = ConfirmDialogWidget::new(&dialog, &palette);
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
