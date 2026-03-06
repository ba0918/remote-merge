//! トップレベル描画関数。

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{AppState, ScanState};
use crate::theme::palette::ensure_contrast;
use crate::ui::dialog::{
    centered_rect, BatchConfirmDialogWidget, ConfirmDialogWidget, DialogState, FilterPanelWidget,
    HelpOverlayWidget, HunkMergePreviewWidget, MtimeWarningDialogWidget, ProgressDialog,
    ServerMenuWidget,
};
use crate::ui::diff_view::DiffView;
use crate::ui::layout::AppLayout;
use crate::ui::tree_view::TreeView;

/// UI を描画する
pub fn draw_ui(frame: &mut Frame, state: &mut AppState) {
    let layout = AppLayout::new(frame.area());

    // フレーム全体にテーマ背景色を塗る（ライトテーマ対応）
    let bg_block = Block::default().style(Style::default().bg(state.palette.bg));
    frame.render_widget(bg_block, frame.area());

    // ビューポートサイズを記録（スクロール計算用）
    state.tree_visible_height = layout.tree_pane.height.saturating_sub(2) as usize;
    state.diff_visible_height = layout.diff_pane.height.saturating_sub(2) as usize;

    draw_header(frame, state, layout.header);
    draw_panes(frame, state, &layout);
    draw_status_bar(frame, state, layout.status_bar);
    draw_dialog(frame, state);
}

/// ヘッダを描画する
fn draw_header(frame: &mut Frame, state: &AppState, area: Rect) {
    let p = &state.palette;
    let conn_indicator = if state.is_connected { "●" } else { "○" };
    let conn_color = if state.is_connected {
        Color::Green
    } else {
        Color::Red
    };

    let local_color = ensure_contrast(Color::Green, p.header_bg);
    let server_color = ensure_contrast(p.badge_modified, p.header_bg);
    let conn_color = ensure_contrast(conn_color, p.header_bg);

    let mut spans = vec![
        Span::styled(
            " remote-merge ",
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw("| "),
        Span::styled("local", Style::default().fg(local_color)),
        Span::raw(" <-> "),
        Span::styled(&state.server_name, Style::default().fg(server_color)),
        Span::raw(" "),
        Span::styled(conn_indicator, Style::default().fg(conn_color)),
    ];

    if state.diff_filter_mode {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            " DIFF ONLY ",
            Style::default()
                .fg(Color::Black)
                .bg(p.badge_modified)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if matches!(state.scan_state, ScanState::Scanning) {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            " SCANNING... ",
            Style::default()
                .fg(Color::Black)
                .bg(p.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let header = Paragraph::new(Line::from(spans))
        .style(Style::default().fg(p.status_bar_fg).bg(p.header_bg));
    frame.render_widget(header, area);
}

/// ツリーペイン + Diff ペインを描画する
fn draw_panes(frame: &mut Frame, state: &AppState, layout: &AppLayout) {
    let tree_view = TreeView::new(state);
    frame.render_widget(tree_view, layout.tree_pane);

    let diff_view = DiffView::new(state);
    frame.render_widget(diff_view, layout.diff_pane);
}

/// ステータスバーを描画する
fn draw_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let p = &state.palette;
    let key_hints = state.build_key_hints();
    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} ", key_hints),
            Style::default().fg(p.accent).bg(p.status_bar_bg),
        ),
        Span::styled("  ", Style::default().bg(p.status_bar_bg)),
        Span::styled(
            &state.status_message,
            Style::default().fg(p.status_bar_fg).bg(p.status_bar_bg),
        ),
    ]))
    .style(Style::default().bg(p.status_bar_bg));
    frame.render_widget(status, area);
}

/// ダイアログを描画する（最前面）
fn draw_dialog(frame: &mut Frame, state: &AppState) {
    match &state.dialog {
        DialogState::Confirm(confirm) => {
            let widget = ConfirmDialogWidget::new(confirm, state.palette.bg);
            frame.render_widget(widget, frame.area());
        }
        DialogState::BatchConfirm(batch) => {
            let widget = BatchConfirmDialogWidget::new(batch, state.palette.bg);
            frame.render_widget(widget, frame.area());
        }
        DialogState::ServerSelect(menu) => {
            let widget = ServerMenuWidget::new(menu, state.palette.bg);
            frame.render_widget(widget, frame.area());
        }
        DialogState::Filter(panel) => {
            let widget = FilterPanelWidget::new(panel, state.palette.bg);
            frame.render_widget(widget, frame.area());
        }
        DialogState::Help(help) => {
            let widget = HelpOverlayWidget::new(help, state.palette.bg);
            frame.render_widget(widget, frame.area());
        }
        DialogState::HunkMergePreview(preview) => {
            let widget = HunkMergePreviewWidget::new(preview, state.palette.bg);
            frame.render_widget(widget, frame.area());
        }
        DialogState::Info(ref msg) => {
            render_info_dialog(frame, msg, state.palette.bg);
        }
        DialogState::Progress(ref progress) => {
            render_progress_dialog(frame, progress, state.palette.bg);
        }
        DialogState::WriteConfirmation => {
            render_simple_dialog(
                frame,
                " Write Changes ",
                &format!("Write {} changes to files?", state.undo_stack.len()),
                Color::Green,
                state.palette.bg,
            );
        }
        DialogState::UnsavedChanges => {
            render_simple_dialog(
                frame,
                " Unsaved Changes ",
                "You have unsaved changes. Discard and quit?",
                Color::Yellow,
                state.palette.bg,
            );
        }
        DialogState::MtimeWarning(ref dialog) => {
            let widget = MtimeWarningDialogWidget {
                dialog,
                border_color: Color::Yellow,
                bg: state.palette.bg,
            };
            frame.render_widget(widget, frame.area());
        }
        DialogState::None => {}
    }
}

/// 情報表示ダイアログ（Esc/Enter で閉じるだけ）
fn render_info_dialog(frame: &mut Frame, message: &str, bg: Color) {
    let dialog_area = centered_rect(60, 7, frame.area());
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Info ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(bg));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let msg = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(message, Style::default().fg(Color::White)),
    ]));
    frame.render_widget(msg, chunks[1]);

    let guide = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "[Enter/Esc]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" OK"),
    ]));
    frame.render_widget(guide, chunks[3]);
}

/// プログレスダイアログを描画する
fn render_progress_dialog(frame: &mut Frame, progress: &ProgressDialog, bg: Color) {
    let dialog_area = centered_rect(50, 8, frame.area());
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(format!(" {} ", progress.title))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(bg));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 空行
            Constraint::Length(1), // プログレスバー or テキスト
            Constraint::Length(1), // 進捗テキスト（件数）
            Constraint::Length(1), // 処理中パス
            Constraint::Length(1), // キャンセルヒント
            Constraint::Min(0),    // 余白
        ])
        .split(inner);

    let bar_width = (chunks[1].width as usize).saturating_sub(4);

    match progress.total {
        Some(total) if total > 0 => {
            // ── total が確定しているケース ──
            let filled = (progress.current as f64 / total as f64 * bar_width as f64) as usize;
            let bar = format!(
                "  [{}{}]",
                "█".repeat(filled.min(bar_width)),
                "░".repeat(bar_width.saturating_sub(filled))
            );
            let bar_para = Paragraph::new(Line::from(Span::styled(
                bar,
                Style::default().fg(Color::Cyan),
            )));
            frame.render_widget(bar_para, chunks[1]);

            let pct = (progress.current as f64 / total as f64 * 100.0).min(100.0);
            let text = format!("{} / {} files ({:.0}%)", progress.current, total, pct);
            let msg = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            frame.render_widget(msg, chunks[2]);
        }
        _ => {
            // ── total 不明（ディレクトリ走査フェーズ）──
            // バウンスアニメーション: current を使って位置をずらす
            let marker_width = 4.min(bar_width);
            let travel = bar_width.saturating_sub(marker_width);
            let pos = if travel > 0 {
                let cycle = travel * 2;
                let raw = progress.current % cycle.max(1);
                if raw < travel {
                    raw
                } else {
                    cycle - raw
                }
            } else {
                0
            };
            let bar = format!(
                "  [{}{}{}]",
                "░".repeat(pos),
                "━".repeat(marker_width),
                "░".repeat(bar_width.saturating_sub(pos + marker_width)),
            );
            let bar_para = Paragraph::new(Line::from(Span::styled(
                bar,
                Style::default().fg(Color::Cyan),
            )));
            frame.render_widget(bar_para, chunks[1]);

            let text = format!("Discovering files... {} found", progress.current);
            let msg = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            frame.render_widget(msg, chunks[2]);
        }
    }

    // 処理中パス表示
    if let Some(ref path) = progress.current_path {
        let max_len = chunks[3].width as usize - 4;
        let display_path = truncate_path(path, max_len);
        let path_para = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(display_path, Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(path_para, chunks[3]);
    }

    // キャンセルヒント
    if progress.cancelable {
        let guide = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "[Esc]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Cancel"),
        ]));
        frame.render_widget(guide, chunks[4]);
    }
}

/// パスが長すぎる場合に先頭を省略する（例: `.../handler/merge_exec.rs`）
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    let suffix = &path[path.len().saturating_sub(max_len.saturating_sub(3))..];
    format!("...{}", suffix)
}

/// シンプルな Y/n 確認ダイアログを描画する
fn render_simple_dialog(frame: &mut Frame, title: &str, message: &str, color: Color, bg: Color) {
    let dialog_area = centered_rect(60, 7, frame.area());
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(bg));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let msg = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(message, Style::default().fg(Color::White)),
    ]));
    frame.render_widget(msg, chunks[1]);

    let guide = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "[Y]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Yes  "),
        Span::styled(
            "[n/Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" No"),
    ]));
    frame.render_widget(guide, chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_path_short() {
        assert_eq!(truncate_path("src/main.rs", 30), "src/main.rs");
    }

    #[test]
    fn test_truncate_path_exact() {
        let path = "src/main.rs"; // 11 chars
        assert_eq!(truncate_path(path, 11), "src/main.rs");
    }

    #[test]
    fn test_truncate_path_long() {
        let path = "src/handler/merge_exec/very_long_name.rs";
        let result = truncate_path(path, 20);
        assert!(result.starts_with("..."));
        assert!(result.len() <= 20);
        assert!(result.ends_with(".rs"));
    }

    #[test]
    fn test_truncate_path_very_short_max() {
        let path = "src/handler/merge_exec.rs";
        let result = truncate_path(path, 5);
        assert!(result.starts_with("..."));
    }

    #[test]
    fn test_progress_dialog_with_current_path() {
        let dialog = ProgressDialog {
            title: "Scanning src/".to_string(),
            current: 5,
            total: None,
            current_path: Some("src/handler/merge_exec.rs".to_string()),
            cancelable: true,
        };
        assert_eq!(
            dialog.current_path.as_deref(),
            Some("src/handler/merge_exec.rs")
        );
    }

    #[test]
    fn test_progress_dialog_content_phase() {
        let dialog = ProgressDialog {
            title: "Loading files...".to_string(),
            current: 10,
            total: Some(46),
            current_path: Some("src/config.rs".to_string()),
            cancelable: false,
        };
        assert_eq!(dialog.total, Some(46));
        assert_eq!(dialog.current, 10);
    }
}
