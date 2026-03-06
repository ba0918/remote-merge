//! mtime 衝突警告ダイアログ。
//!
//! マージ実行直前にリモート/ローカルの mtime が変更されていた場合に表示する。
//! ユーザーは [r]eload / [f]orce / [c]ancel から選択する。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::merge::optimistic_lock::MtimeConflict;
use crate::ui::metadata::format_mtime;

use super::render_dialog_frame;

/// mtime 警告ダイアログの状態
#[derive(Debug, Clone)]
pub struct MtimeWarningDialog {
    /// 衝突したファイルのリスト
    pub conflicts: Vec<MtimeConflict>,
    /// 元のマージ操作を再試行するための情報
    pub merge_context: MtimeWarningMergeContext,
}

/// 警告ダイアログから復帰するために必要なマージコンテキスト
#[derive(Debug, Clone)]
pub enum MtimeWarningMergeContext {
    /// 単一ファイルマージ
    Single {
        path: String,
        direction: crate::merge::executor::MergeDirection,
    },
    /// バッチマージ
    Batch {
        direction: crate::merge::executor::MergeDirection,
    },
}

/// mtime 警告ダイアログ Widget
pub struct MtimeWarningDialogWidget<'a> {
    pub dialog: &'a MtimeWarningDialog,
    pub border_color: Color,
    pub bg: Color,
}

impl<'a> Widget for MtimeWarningDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let conflict_count = self.dialog.conflicts.len();
        let height = (conflict_count as u16 * 3 + 7).min(area.height);
        let width = 56u16.min(area.width);

        let inner = render_dialog_frame(
            " ⚠ File Changed ",
            self.border_color,
            width,
            height,
            area,
            buf,
            self.bg,
        );

        let mut lines = Vec::new();

        lines.push(Line::from(Span::styled(
            "The following file(s) have been modified since diff:",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        for conflict in &self.dialog.conflicts {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    &conflict.path,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    diff: "),
                Span::styled(
                    format_mtime(conflict.expected),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw("  now: "),
                Span::styled(
                    format_mtime(conflict.actual),
                    Style::default().fg(Color::Red),
                ),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                " [r]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("eload  "),
            Span::styled(
                "[f]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("orce  "),
            Span::styled(
                "[c]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("ancel"),
        ]));

        Paragraph::new(lines).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_mtime_warning_dialog_renders() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 23, 0).unwrap();

        let dialog = MtimeWarningDialog {
            conflicts: vec![MtimeConflict {
                path: "src/config.ts".to_string(),
                expected: Some(dt1),
                actual: Some(dt2),
            }],
            merge_context: MtimeWarningMergeContext::Single {
                path: "src/config.ts".to_string(),
                direction: crate::merge::executor::MergeDirection::LocalToRemote,
            },
        };

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        let widget = MtimeWarningDialogWidget {
            dialog: &dialog,
            border_color: Color::Yellow,
            bg: Color::Black,
        };

        widget.render(area, &mut buf);

        // ダイアログが描画されたことを確認（バッファにコンテンツがある）
        let content = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(content.contains("config.ts"));
    }
}
