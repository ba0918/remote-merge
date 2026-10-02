//! mtime 衝突警告ダイアログ（Widget のみ。データ型は app/dialog_types.rs）。
//!
//! マージ実行直前にリモート/ローカルの mtime が変更されていた場合に表示する。
//! ユーザーは [r]eload / [f]orce / [c]ancel から選択する。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::dialog_types::MtimeWarningDialog;
use crate::merge::optimistic_lock::ConflictReason;
use crate::ui::metadata::format_mtime;

use crate::theme::palette::TuiPalette;

use super::render_dialog_frame;

/// mtime 警告ダイアログ Widget
pub struct MtimeWarningDialogWidget<'a> {
    pub dialog: &'a MtimeWarningDialog,
    pub palette: &'a TuiPalette,
}

impl<'a> Widget for MtimeWarningDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let conflict_count = self.dialog.conflicts.len();
        let height = (conflict_count as u16 * 3 + 7).min(area.height);
        let width = 60u16.min(area.width);

        let inner = render_dialog_frame(
            " ⚠ File Changed ",
            self.palette.dialog_accent,
            width,
            height,
            area,
            buf,
            self.palette.bg,
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
            match conflict.reason {
                ConflictReason::FileDeleted => {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "FILE DELETED",
                            Style::default()
                                .fg(self.palette.negative)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" (was: "),
                        Span::styled(
                            format_mtime(conflict.expected),
                            Style::default().fg(self.palette.dialog_accent),
                        ),
                        Span::raw(")"),
                    ]));
                }
                ConflictReason::Changed => {
                    lines.push(Line::from(vec![
                        Span::raw("    diff: "),
                        Span::styled(
                            format_mtime(conflict.expected),
                            Style::default().fg(self.palette.dialog_accent),
                        ),
                        Span::raw("  now: "),
                        Span::styled(
                            format_mtime(conflict.actual),
                            Style::default().fg(self.palette.negative),
                        ),
                    ]));
                }
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                " [r]",
                Style::default()
                    .fg(self.palette.positive)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("eload  "),
            Span::styled(
                "[f]",
                Style::default()
                    .fg(self.palette.dialog_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("orce  "),
            Span::styled(
                "[c]",
                Style::default()
                    .fg(self.palette.negative)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("ancel"),
        ]));

        Paragraph::new(lines).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::dialog_types::MtimeWarningMergeContext;
    use crate::merge::optimistic_lock::MtimeConflict;
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
                reason: ConflictReason::Changed,
            }],
            merge_context: MtimeWarningMergeContext::Single {
                path: "src/config.ts".to_string(),
                direction: crate::merge::executor::MergeDirection::LeftToRight,
            },
        };

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette = TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = MtimeWarningDialogWidget {
            dialog: &dialog,
            palette: &palette,
        };

        widget.render(area, &mut buf);

        // ダイアログが描画されたことを確認（バッファにコンテンツがある）
        let content = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(content.contains("config.ts"));
    }

    /// mtime の日時が切れずに表示されることを確認する。
    /// ダイアログ内部幅が "    diff: YYYY-MM-DD HH:MM:SS  now: YYYY-MM-DD HH:MM:SS" (55文字)
    /// を収容できること。
    #[test]
    fn test_mtime_warning_dialog_width_shows_full_timestamps() {
        let dt1 = Utc.with_ymd_and_hms(2026, 3, 6, 14, 35, 47).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2026, 3, 6, 14, 35, 49).unwrap();

        let dialog = MtimeWarningDialog {
            conflicts: vec![MtimeConflict {
                path: "src/diff/engine.rs".to_string(),
                expected: Some(dt1),
                actual: Some(dt2),
                reason: ConflictReason::Changed,
            }],
            merge_context: MtimeWarningMergeContext::Single {
                path: "src/diff/engine.rs".to_string(),
                direction: crate::merge::executor::MergeDirection::LeftToRight,
            },
        };

        // ダイアログ幅(60) + 左右ボーダー(2) = 最低62幅、十分な領域で描画
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette = TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = MtimeWarningDialogWidget {
            dialog: &dialog,
            palette: &palette,
        };
        widget.render(area, &mut buf);

        // バッファから行を抽出して "now:" の後の秒まで表示されているか確認
        let mut found_now_line = false;
        for y in 0..area.height {
            let row: String = (0..area.width)
                .map(|x| {
                    buf.cell((x, y))
                        .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
                })
                .collect();
            if row.contains("now:") {
                found_now_line = true;
                // "now:" の後にタイムスタンプ末尾の秒(2桁)まで含まれていること
                // format_mtime は Local タイムゾーンに変換するが、秒部分が切れていないことを確認
                let now_pos = row.find("now:").unwrap();
                let after_now = &row[now_pos..];
                // "now: YYYY-MM-DD HH:MM:SS" → "now:" の後に少なくとも20文字
                let trimmed = after_now.trim_end();
                assert!(
                    trimmed.len() >= 24,
                    "Timestamp after 'now:' is truncated: '{}'",
                    trimmed,
                );
            }
        }
        assert!(found_now_line, "Could not find 'now:' in rendered dialog");
    }

    #[test]
    fn test_mtime_warning_dialog_renders_file_deleted() {
        let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();

        let dialog = MtimeWarningDialog {
            conflicts: vec![MtimeConflict {
                path: "deleted_file.rs".to_string(),
                expected: Some(dt1),
                actual: None,
                reason: ConflictReason::FileDeleted,
            }],
            merge_context: MtimeWarningMergeContext::Single {
                path: "deleted_file.rs".to_string(),
                direction: crate::merge::executor::MergeDirection::LeftToRight,
            },
        };

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette = TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = MtimeWarningDialogWidget {
            dialog: &dialog,
            palette: &palette,
        };
        widget.render(area, &mut buf);

        let content = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(content.contains("deleted_file.rs"));
        assert!(content.contains("FILE DELETED"));
    }
}
