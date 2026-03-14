//! バッチマージ確認ダイアログ。

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::Badge;
use crate::merge::executor::MergeDirection;
use crate::theme::palette::TuiPalette;

use super::render_dialog_frame;

/// バッチマージ確認ダイアログの状態
#[derive(Debug, Clone)]
pub struct BatchConfirmDialog {
    /// 対象ファイルとバッジ一覧
    pub files: Vec<(String, Badge)>,
    /// マージ方向
    pub direction: MergeDirection,
    /// ソース名
    pub source_name: String,
    /// ターゲット名
    pub target_name: String,
    /// スクロール位置（大量ファイル対応）
    pub scroll: usize,
    /// 未比較(Unchecked)ディレクトリ数（警告用）
    pub unchecked_count: usize,
    /// センシティブファイル一覧
    pub sensitive_files: Vec<String>,
}

impl BatchConfirmDialog {
    pub fn new(
        files: Vec<(String, Badge)>,
        direction: MergeDirection,
        source_name: String,
        target_name: String,
        unchecked_count: usize,
    ) -> Self {
        Self {
            files,
            direction,
            source_name,
            target_name,
            scroll: 0,
            unchecked_count,
            sensitive_files: Vec::new(),
        }
    }

    /// センシティブファイルパターンでチェックを行い、マッチするファイルを記録する
    pub fn check_sensitive(&mut self, patterns: &[String]) {
        self.sensitive_files = self
            .files
            .iter()
            .filter(|(path, _)| {
                let filename = path.rsplit('/').next().unwrap_or(path);
                patterns
                    .iter()
                    .any(|p| glob_match::glob_match(p, filename) || glob_match::glob_match(p, path))
            })
            .map(|(path, _)| path.clone())
            .collect();
    }

    /// 大量ファイル（21件以上）かどうか
    pub fn is_large_batch(&self) -> bool {
        self.files.len() > 20
    }

    /// メッセージを生成
    pub fn message(&self) -> String {
        format!(
            "Merge {} file(s) from {} → {}",
            self.files.len(),
            self.source_name,
            self.target_name
        )
    }

    /// スクロールダウン
    pub fn scroll_down(&mut self) {
        if self.scroll + 1 < self.files.len() {
            self.scroll += 1;
        }
    }

    /// スクロールアップ
    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }
}

/// バッチマージ確認ダイアログウィジェット
pub struct BatchConfirmDialogWidget<'a> {
    dialog: &'a BatchConfirmDialog,
    palette: &'a TuiPalette,
}

impl<'a> BatchConfirmDialogWidget<'a> {
    pub fn new(dialog: &'a BatchConfirmDialog, palette: &'a TuiPalette) -> Self {
        Self { dialog, palette }
    }
}

impl<'a> Widget for BatchConfirmDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let file_count = self.dialog.files.len();
        let warning_lines = 1 // mtime未チェック警告（常に表示）
            + if self.dialog.unchecked_count > 0 {
                1
            } else {
                0
            }
            + if !self.dialog.sensitive_files.is_empty() {
                1
            } else {
                0
            };
        let visible_files = file_count.min(15);
        let height = (visible_files as u16) + (warning_lines as u16) + 6;
        let width = area.width.min(70);
        let title = format!(" Batch Merge ({} files) ", file_count);
        let inner = render_dialog_frame(
            &title,
            self.palette.dialog_accent,
            width,
            height,
            area,
            buf,
            self.palette.bg,
        );

        let mut constraints: Vec<Constraint> = Vec::new();
        constraints.push(Constraint::Length(1)); // メッセージ行
        constraints.push(Constraint::Length(1)); // mtime未チェック警告
        if self.dialog.unchecked_count > 0 {
            constraints.push(Constraint::Length(1));
        }
        if !self.dialog.sensitive_files.is_empty() {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Length(1)); // 空行
        for _ in 0..visible_files {
            constraints.push(Constraint::Length(1));
        }
        if file_count > visible_files {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Length(1)); // 空行
        constraints.push(Constraint::Length(1)); // ガイド行

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut row = 0;

        // メッセージ行
        let msg = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(self.dialog.message(), Style::default().fg(Color::White)),
        ]));
        msg.render(chunks[row], buf);
        row += 1;

        // 差分自動チェックの案内
        let has_unchecked = self
            .dialog
            .files
            .iter()
            .any(|(_, b)| *b == Badge::Unchecked);
        let caution_msg = if has_unchecked {
            "Unchecked files will be auto-verified; identical files skipped"
        } else {
            "All files have been diff-checked"
        };
        let caution_color = if has_unchecked {
            self.palette.dialog_accent
        } else {
            Color::Green
        };
        let caution = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(caution_msg, Style::default().fg(caution_color)),
        ]));
        caution.render(chunks[row], buf);
        row += 1;

        // 未比較警告
        if self.dialog.unchecked_count > 0 {
            let warn = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "⚠ {} unchecked director(ies) found",
                        self.dialog.unchecked_count
                    ),
                    Style::default().fg(self.palette.dialog_accent),
                ),
            ]));
            warn.render(chunks[row], buf);
            row += 1;
        }

        // センシティブ警告
        if !self.dialog.sensitive_files.is_empty() {
            let warn = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "⚠ {} sensitive file(s) included",
                        self.dialog.sensitive_files.len()
                    ),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]));
            warn.render(chunks[row], buf);
            row += 1;
        }

        row += 1; // 空行

        // ファイル一覧（スクロール対応）
        let start = self.dialog.scroll;
        let end = (start + visible_files).min(file_count);
        for i in start..end {
            if let Some((path, badge)) = self.dialog.files.get(i) {
                let badge_style = match badge {
                    Badge::Modified => Style::default().fg(self.palette.badge_modified),
                    Badge::LeftOnly => Style::default().fg(self.palette.badge_left_only),
                    Badge::RightOnly => Style::default().fg(self.palette.badge_right_only),
                    _ => Style::default().fg(Color::White),
                };
                let is_sensitive = self.dialog.sensitive_files.contains(path);
                let sensitive_mark = if is_sensitive { " ⚠" } else { "" };

                let line = Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(badge.label(), badge_style),
                    Span::raw(" "),
                    Span::styled(path.as_str(), Style::default().fg(Color::White)),
                    Span::styled(sensitive_mark, Style::default().fg(Color::Red)),
                ]));
                line.render(chunks[row], buf);
            }
            row += 1;
        }

        // "...and N more" 表示
        if file_count > visible_files {
            let more = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "  ...and {} more (j/k to scroll)",
                        file_count - visible_files
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            more.render(chunks[row], buf);
            row += 1;
        }

        row += 1; // 空行

        // ガイド行
        if row < chunks.len() {
            let suffix = if self.dialog.is_large_batch() {
                Some(("(large batch)", self.palette.dialog_accent))
            } else {
                None
            };
            let guide = Paragraph::new(super::confirm_cancel_guide(suffix));
            guide.render(chunks[row], buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_confirm_dialog_message() {
        let batch = BatchConfirmDialog::new(
            vec![
                ("src/a.ts".to_string(), Badge::Modified),
                ("src/b.ts".to_string(), Badge::LeftOnly),
            ],
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
            1,
        );
        assert_eq!(batch.message(), "Merge 2 file(s) from local → develop");
        assert!(!batch.is_large_batch());
        assert_eq!(batch.unchecked_count, 1);
    }

    #[test]
    fn test_batch_confirm_dialog_scroll() {
        let mut batch = BatchConfirmDialog::new(
            (0..25)
                .map(|i| (format!("file{}.txt", i), Badge::Modified))
                .collect(),
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
            0,
        );

        assert_eq!(batch.scroll, 0);
        batch.scroll_down();
        assert_eq!(batch.scroll, 1);
        batch.scroll_up();
        assert_eq!(batch.scroll, 0);
        batch.scroll_up();
        assert_eq!(batch.scroll, 0);
    }

    #[test]
    fn test_batch_confirm_dialog_render() {
        let batch = BatchConfirmDialog::new(
            vec![
                ("src/a.ts".to_string(), Badge::Modified),
                ("src/b.ts".to_string(), Badge::LeftOnly),
            ],
            MergeDirection::LeftToRight,
            "local".to_string(),
            "develop".to_string(),
            0,
        );

        let area = Rect::new(0, 0, 80, 30);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let palette = TuiPalette::from_theme(&ts.themes["base16-ocean.dark"]);
        let widget = BatchConfirmDialogWidget::new(&batch, &palette);
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

        assert!(content.contains("Batch Merge"));
        assert!(content.contains("[M]"));
    }
}
