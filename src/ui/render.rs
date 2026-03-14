//! トップレベル描画関数。

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{AppState, ScanState};
use crate::theme::palette::ensure_contrast;
use crate::ui::dialog::{
    centered_rect, BatchConfirmDialogWidget, ConfirmDialogWidget, DialogState, FilterPanelWidget,
    HelpOverlayWidget, HunkMergePreviewWidget, MtimeWarningDialogWidget, PairServerMenuWidget,
    ProgressDialog, ProgressPhase, ServerMenuWidget, ThreeWaySummaryWidget,
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

    let left_name = state.left_source.display_name();
    let right_name = state.right_source.display_name();

    let mut spans = vec![
        Span::styled(
            " remote-merge ",
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw("| "),
        Span::styled(left_name, Style::default().fg(local_color)),
        Span::raw(" <-> "),
        Span::styled(right_name, Style::default().fg(server_color)),
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

/// 右側サーバの Agent 接続状態を示すテキストを返す
fn agent_status_text(state: &AppState) -> &'static str {
    match state.right_source.server_name() {
        Some(name) if state.agent_connected_servers.contains(name) => "Agent: connected",
        Some(_) => "Fallback: SSH exec",
        None => "", // ローカル同士の比較では表示しない
    }
}

/// ステータスバーを描画する
fn draw_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let p = &state.palette;
    let key_hints = state.build_key_hints();
    let agent_text = agent_status_text(state);
    let mut spans = vec![
        Span::styled(
            format!(" {} ", key_hints),
            Style::default().fg(p.accent).bg(p.status_bar_bg),
        ),
        Span::styled("  ", Style::default().bg(p.status_bar_bg)),
        Span::styled(
            &state.status_message,
            Style::default().fg(p.status_bar_fg).bg(p.status_bar_bg),
        ),
    ];

    if !agent_text.is_empty() {
        spans.push(Span::styled(
            "  | ",
            Style::default().fg(p.status_bar_fg).bg(p.status_bar_bg),
        ));
        let agent_color = if agent_text.starts_with("Agent:") {
            Color::Green
        } else {
            state.palette.dialog_accent
        };
        let agent_color = crate::theme::palette::ensure_contrast(agent_color, p.status_bar_bg);
        spans.push(Span::styled(
            agent_text,
            Style::default().fg(agent_color).bg(p.status_bar_bg),
        ));
    }

    let status = Paragraph::new(Line::from(spans)).style(Style::default().bg(p.status_bar_bg));
    frame.render_widget(status, area);
}

/// ダイアログを描画する（最前面）
fn draw_dialog(frame: &mut Frame, state: &AppState) {
    match &state.dialog {
        DialogState::Confirm(confirm) => {
            let widget = ConfirmDialogWidget::new(confirm, &state.palette);
            frame.render_widget(widget, frame.area());
        }
        DialogState::BatchConfirm(batch) => {
            let widget = BatchConfirmDialogWidget::new(batch, &state.palette);
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
            let widget = HelpOverlayWidget::new(help, &state.palette);
            frame.render_widget(widget, frame.area());
        }
        DialogState::HunkMergePreview(preview) => {
            let widget = HunkMergePreviewWidget::new(preview, &state.palette);
            frame.render_widget(widget, frame.area());
        }
        DialogState::Info(ref msg) => {
            render_info_dialog(frame, msg, state.palette.bg);
        }
        DialogState::Progress(ref progress) => {
            render_progress_dialog(
                frame,
                progress,
                state.palette.bg,
                state.palette.dialog_accent,
            );
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
                state.palette.dialog_accent,
                state.palette.bg,
            );
        }
        DialogState::MtimeWarning(ref dialog) => {
            let widget = MtimeWarningDialogWidget {
                dialog,
                palette: &state.palette,
            };
            frame.render_widget(widget, frame.area());
        }
        DialogState::PairServerSelect(menu) => {
            let widget = PairServerMenuWidget::new(menu, state.palette.bg);
            frame.render_widget(widget, frame.area());
        }
        DialogState::ThreeWaySummary(ref panel) => {
            let widget = ThreeWaySummaryWidget::new(panel, &state.palette);
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

    let guide = Paragraph::new(crate::ui::dialog::ok_guide());
    frame.render_widget(guide, chunks[3]);
}

/// プログレスダイアログを描画する
fn render_progress_dialog(frame: &mut Frame, progress: &ProgressDialog, bg: Color, accent: Color) {
    let dialog_area = centered_rect(50, 8, frame.area());
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(format!(" {} ", progress.display_title()))
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

    let (bar, text) =
        compute_progress_bar(progress.current, progress.total, bar_width, &progress.phase);

    let bar_para = Paragraph::new(Line::from(Span::styled(
        bar,
        Style::default().fg(Color::Cyan),
    )));
    frame.render_widget(bar_para, chunks[1]);

    let msg = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            text,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(msg, chunks[2]);

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
        let guide = Paragraph::new(crate::ui::dialog::cancel_guide());
        frame.render_widget(guide, chunks[4]);
    }
}

/// パスが長すぎる場合に先頭を省略する（例: `.../handler/merge_exec.rs`）
///
/// UTF-8 文字境界を考慮し、マルチバイト文字を含むパスでもパニックしない。
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.chars().count() <= max_len {
        return path.to_string();
    }
    let suffix_len = max_len.saturating_sub(3);
    let skip = path.chars().count().saturating_sub(suffix_len);
    let suffix: String = path.chars().skip(skip).collect();
    format!("...{}", suffix)
}

/// 進捗バーの表示文字列とテキストを生成する（純粋関数）
///
/// `total` が `Some(n)` (n > 0) の場合は確定プログレスバー、
/// それ以外はバウンスアニメーション（不定形式）を返す。
fn compute_progress_bar(
    current: usize,
    total: Option<usize>,
    bar_width: usize,
    phase: &ProgressPhase,
) -> (String, String) {
    match total {
        Some(total) if total > 0 => {
            let filled = (current as f64 / total as f64 * bar_width as f64) as usize;
            let bar = format!(
                "  [{}{}]",
                "█".repeat(filled.min(bar_width)),
                "░".repeat(bar_width.saturating_sub(filled))
            );
            let pct = (current as f64 / total as f64 * 100.0).min(100.0);
            let text = format!("{} / {} files ({:.0}%)", current, total, pct);
            (bar, text)
        }
        _ => {
            // バウンスアニメーション: current を使って位置をずらす
            let marker_width = 4.min(bar_width);
            let travel = bar_width.saturating_sub(marker_width);
            let pos = if travel > 0 {
                let cycle = travel * 2;
                let raw = current % cycle.max(1);
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
            let text = phase.indeterminate_text(current);
            (bar, text)
        }
    }
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

    let guide = Paragraph::new(crate::ui::dialog::confirm_cancel_guide(None));
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
        assert!(result.chars().count() <= 20);
        assert!(result.ends_with(".rs"));
    }

    #[test]
    fn test_truncate_path_very_short_max() {
        let path = "src/handler/merge_exec.rs";
        let result = truncate_path(path, 5);
        assert!(result.starts_with("..."));
    }

    #[test]
    fn test_truncate_path_multibyte() {
        // マルチバイト文字を含むパスでもパニックしない
        let path = "src/日本語ディレクトリ/設定.rs";
        let result = truncate_path(path, 10);
        assert!(result.starts_with("..."));
        assert!(result.chars().count() <= 10);
        assert!(result.ends_with(".rs"));
    }

    #[test]
    fn test_agent_status_text_connected() {
        let mut state = crate::app::AppState::new(
            crate::tree::FileTree::new("/test"),
            crate::tree::FileTree::new("/test"),
            crate::app::Side::Local,
            crate::app::Side::Remote("develop".to_string()),
            "default",
        );
        state.agent_connected_servers.insert("develop".to_string());
        assert_eq!(agent_status_text(&state), "Agent: connected");
    }

    #[test]
    fn test_agent_status_text_fallback() {
        let state = crate::app::AppState::new(
            crate::tree::FileTree::new("/test"),
            crate::tree::FileTree::new("/test"),
            crate::app::Side::Local,
            crate::app::Side::Remote("develop".to_string()),
            "default",
        );
        assert_eq!(agent_status_text(&state), "Fallback: SSH exec");
    }

    #[test]
    fn test_agent_status_text_local() {
        let state = crate::app::AppState::new(
            crate::tree::FileTree::new("/test"),
            crate::tree::FileTree::new("/test"),
            crate::app::Side::Local,
            crate::app::Side::Local,
            "default",
        );
        assert_eq!(agent_status_text(&state), "");
    }

    #[test]
    fn test_progress_dialog_with_current_path() {
        use crate::ui::dialog::ProgressPhase;
        let mut dialog = ProgressDialog::new(ProgressPhase::Scanning, "src/", true);
        dialog.current = 5;
        dialog.current_path = Some("src/handler/merge_exec.rs".to_string());
        assert_eq!(
            dialog.current_path.as_deref(),
            Some("src/handler/merge_exec.rs")
        );
        assert_eq!(dialog.display_title(), "Scanning src/");
    }

    #[test]
    fn test_progress_dialog_content_phase() {
        use crate::ui::dialog::ProgressPhase;
        let mut dialog = ProgressDialog::new(ProgressPhase::LoadingFiles, "", false);
        dialog.current = 10;
        dialog.total = Some(46);
        dialog.current_path = Some("src/config.rs".to_string());
        assert_eq!(dialog.total, Some(46));
        assert_eq!(dialog.current, 10);
        assert_eq!(dialog.display_title(), "Loading files");
    }

    // --- truncate_path エッジケース ---

    #[test]
    fn test_truncate_path_empty_string() {
        assert_eq!(truncate_path("", 10), "");
    }

    #[test]
    fn test_truncate_path_max_len_one() {
        // max_len=1 → suffix_len = 1-3 = 0 (saturating_sub)
        // skip = 全文字数 → suffix は空 → "..."
        let result = truncate_path("src/main.rs", 1);
        assert!(result.starts_with("..."));
    }

    #[test]
    fn test_truncate_path_max_len_zero() {
        let result = truncate_path("src/main.rs", 0);
        assert!(result.starts_with("..."));
    }

    // --- compute_progress_bar テスト ---

    #[test]
    fn test_progress_bar_zero_percent() {
        let (bar, text) = compute_progress_bar(0, Some(10), 20, &ProgressPhase::LoadingFiles);
        // 0% → filled=0, 全て ░
        assert!(bar.contains('['));
        assert!(bar.contains(']'));
        assert!(text.contains("0 / 10 files (0%)"));
    }

    #[test]
    fn test_progress_bar_fifty_percent() {
        let (bar, text) = compute_progress_bar(5, Some(10), 20, &ProgressPhase::LoadingFiles);
        assert!(text.contains("5 / 10 files (50%)"));
        // filled=10, bar_width=20 → 10 █ と 10 ░
        assert!(bar.contains("██████████"));
        assert!(bar.contains("░░░░░░░░░░"));
    }

    #[test]
    fn test_progress_bar_hundred_percent() {
        let (bar, text) = compute_progress_bar(10, Some(10), 20, &ProgressPhase::LoadingFiles);
        assert!(text.contains("10 / 10 files (100%)"));
        // 全て █
        assert!(bar.contains("████████████████████"));
        assert!(!bar.contains('░'));
    }

    #[test]
    fn test_progress_bar_over_hundred_percent() {
        // current > total の場合も 100% を超えない
        let (_bar, text) = compute_progress_bar(15, Some(10), 20, &ProgressPhase::LoadingFiles);
        assert!(text.contains("100%"));
    }

    #[test]
    fn test_progress_bar_total_zero_division() {
        // total=Some(0) → _ ブランチへフォールバック（ゼロ除算回避）
        let (bar, text) = compute_progress_bar(5, Some(0), 20, &ProgressPhase::Scanning);
        assert!(bar.contains('['));
        assert!(text.contains("Discovering files... 5 found"));
    }

    #[test]
    fn test_progress_bar_total_none_indeterminate() {
        let (bar, text) = compute_progress_bar(0, None, 20, &ProgressPhase::Scanning);
        // バウンスアニメーション
        assert!(bar.contains("━"));
        assert!(text.contains("Discovering files... 0 found"));
    }

    #[test]
    fn test_progress_bar_indeterminate_bounce_forward() {
        // current=3, travel=16, cycle=32, raw=3 < 16 → pos=3
        let (bar, _text) = compute_progress_bar(3, None, 20, &ProgressPhase::Scanning);
        assert!(bar.contains("━━━━"));
    }

    #[test]
    fn test_progress_bar_indeterminate_bounce_backward() {
        // bar_width=20, marker_width=4, travel=16, cycle=32
        // current=20 → raw=20 % 32 = 20 ≥ 16 → pos = 32 - 20 = 12
        let (bar, _text) = compute_progress_bar(20, None, 20, &ProgressPhase::Scanning);
        assert!(bar.contains("━━━━"));
    }

    #[test]
    fn test_progress_bar_bar_width_zero() {
        // bar_width=0 → marker_width=0, travel=0, pos=0
        let (bar, text) = compute_progress_bar(5, None, 0, &ProgressPhase::Scanning);
        assert_eq!(bar, "  []");
        assert!(text.contains("Discovering files... 5 found"));
    }

    #[test]
    fn test_progress_bar_bar_width_zero_with_total() {
        let (bar, text) = compute_progress_bar(5, Some(10), 0, &ProgressPhase::LoadingFiles);
        assert_eq!(bar, "  []");
        assert!(text.contains("5 / 10 files (50%)"));
    }

    #[test]
    fn test_progress_bar_small_bar_width() {
        // bar_width=3 → marker_width=3, travel=0, pos=0
        let (bar, _text) = compute_progress_bar(0, None, 3, &ProgressPhase::Scanning);
        assert!(bar.contains("━━━"));
    }

    // --- ProgressPhase テスト ---

    #[test]
    fn test_progress_phase_titles() {
        assert_eq!(ProgressPhase::Scanning.title(), "Scanning");
        assert_eq!(ProgressPhase::LoadingFiles.title(), "Loading files");
        assert_eq!(ProgressPhase::LoadingRemote.title(), "Loading remote files");
        assert_eq!(ProgressPhase::Merging.title(), "Merging");
    }

    #[test]
    fn test_progress_phase_indeterminate_text() {
        assert_eq!(
            ProgressPhase::Scanning.indeterminate_text(42),
            "Discovering files... 42 found"
        );
        assert_eq!(
            ProgressPhase::LoadingFiles.indeterminate_text(7),
            "Processing... 7"
        );
        assert_eq!(
            ProgressPhase::LoadingRemote.indeterminate_text(0),
            "Processing... 0"
        );
        assert_eq!(
            ProgressPhase::Merging.indeterminate_text(3),
            "Processing... 3"
        );
    }

    // --- ProgressDialog メソッドテスト ---

    #[test]
    fn test_progress_dialog_display_title_with_context() {
        let dialog = ProgressDialog::new(ProgressPhase::Scanning, "/var/www", true);
        assert_eq!(dialog.display_title(), "Scanning /var/www");
    }

    #[test]
    fn test_progress_dialog_display_title_empty_context() {
        let dialog = ProgressDialog::new(ProgressPhase::Merging, "", false);
        assert_eq!(dialog.display_title(), "Merging");
    }

    #[test]
    fn test_progress_dialog_new_defaults() {
        let dialog = ProgressDialog::new(ProgressPhase::LoadingRemote, "server", true);
        assert_eq!(dialog.current, 0);
        assert_eq!(dialog.total, None);
        assert_eq!(dialog.current_path, None);
        assert!(dialog.cancelable);
        assert_eq!(dialog.phase, ProgressPhase::LoadingRemote);
        assert_eq!(dialog.context, "server");
    }
}
