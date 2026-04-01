//! モーダルダイアログ（確認ダイアログ + サーバ選択メニュー等）。

mod batch_confirm;
mod confirm;
mod filter_panel;
mod help;
mod hunk_preview;
mod mtime_warning;
mod pair_server_menu;
mod server_menu;
mod three_way_summary;

// ドメイン層のダイアログ型を re-export（後方互換性のため）
// 正規の定義は app/dialog_types.rs にある。
pub use crate::app::dialog_types::{
    BatchConfirmDialog, Column, ConfirmDialog, DialogState, FilterPanel, HelpOverlay, HelpSection,
    HunkMergePreview, MtimeWarningDialog, MtimeWarningMergeContext, PairServerMenu, ProgressDialog,
    ProgressPhase, ServerMenu,
};

// Widget 型は ui/dialog/ の各サブモジュールで定義
pub use batch_confirm::BatchConfirmDialogWidget;
pub use confirm::ConfirmDialogWidget;
pub use filter_panel::FilterPanelWidget;
pub use help::HelpOverlayWidget;
pub use hunk_preview::HunkMergePreviewWidget;
pub use mtime_warning::MtimeWarningDialogWidget;
pub use pair_server_menu::PairServerMenuWidget;
pub use server_menu::ServerMenuWidget;
pub use three_way_summary::ThreeWaySummaryWidget;

use crate::theme::palette::TuiPalette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};

/// `[Y] Confirm  [n/Esc] Cancel` フッターガイドを生成する。
///
/// `suffix` が `Some` の場合、末尾に追加テキストを付与する（例: "(large batch)"）。
pub fn confirm_cancel_guide(palette: &TuiPalette, suffix: Option<(&str, Color)>) -> Line<'static> {
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(
            "[Y]",
            Style::default()
                .fg(palette.positive)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Confirm  "),
        Span::styled(
            "[n/Esc]",
            Style::default()
                .fg(palette.negative)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ];
    if let Some((text, color)) = suffix {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(text.to_string(), Style::default().fg(color)));
    }
    Line::from(spans)
}

/// `[Enter/Esc] OK` フッターガイドを生成する。
pub fn ok_guide(palette: &TuiPalette) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "[Enter/Esc]",
            Style::default()
                .fg(palette.info)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" OK"),
    ])
}

/// `[Esc] Cancel` フッターガイドを生成する。
pub fn cancel_guide(palette: &TuiPalette) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(palette.negative)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ])
}

/// 中央にモーダルエリアを計算する
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// ダイアログ共通フレームを描画し、内部領域を返す。
/// `bg` でダイアログ内部の背景色を指定する（ライトテーマ対応）。
pub fn render_dialog_frame(
    title: &str,
    border_color: Color,
    width: u16,
    height: u16,
    area: Rect,
    buf: &mut Buffer,
    bg: Color,
) -> Rect {
    let dialog_area = centered_rect(width, height, area);
    Clear.render(dialog_area, buf);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(bg));
    let inner = block.inner(dialog_area);
    block.render(dialog_area, buf);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(rect.width, 30);
        assert_eq!(rect.height, 5);
    }

    #[test]
    fn test_dialog_state_default() {
        let state = DialogState::default();
        assert!(matches!(state, DialogState::None));
    }

    #[test]
    fn test_progress_phase_titles() {
        assert_eq!(ProgressPhase::Scanning.title(), "Scanning");
        assert_eq!(ProgressPhase::LoadingFiles.title(), "Loading files");
        assert_eq!(ProgressPhase::LoadingRemote.title(), "Loading remote files");
        assert_eq!(ProgressPhase::Merging.title(), "Merging");
    }

    #[test]
    fn test_progress_phase_indeterminate_text() {
        let text = ProgressPhase::Scanning.indeterminate_text(42);
        assert_eq!(text, "Discovering files... 42 found");

        let text = ProgressPhase::Merging.indeterminate_text(10);
        assert_eq!(text, "Processing... 10");
    }

    #[test]
    fn test_progress_dialog_new_defaults() {
        let dialog = ProgressDialog::new(ProgressPhase::Scanning, "src/", true);
        assert_eq!(dialog.phase, ProgressPhase::Scanning);
        assert_eq!(dialog.context, "src/");
        assert_eq!(dialog.current, 0);
        assert_eq!(dialog.total, None);
        assert_eq!(dialog.current_path, None);
        assert!(dialog.cancelable);
    }

    #[test]
    fn test_progress_dialog_display_title() {
        let dialog = ProgressDialog::new(ProgressPhase::Scanning, "src/app", true);
        assert_eq!(dialog.display_title(), "Scanning src/app");

        let dialog = ProgressDialog::new(ProgressPhase::LoadingFiles, "", false);
        assert_eq!(dialog.display_title(), "Loading files");
    }

    fn test_palette() -> TuiPalette {
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        TuiPalette::from_theme(&ts.themes["base16-ocean.dark"])
    }

    #[test]
    fn test_confirm_cancel_guide_without_suffix() {
        let palette = test_palette();
        let line = confirm_cancel_guide(&palette, None);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[Y]"));
        assert!(text.contains("Confirm"));
        assert!(text.contains("[n/Esc]"));
        assert!(text.contains("Cancel"));
    }

    #[test]
    fn test_confirm_cancel_guide_with_suffix() {
        let palette = test_palette();
        let line = confirm_cancel_guide(&palette, Some(("(large batch)", Color::Yellow)));
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("(large batch)"));
    }

    #[test]
    fn test_ok_guide_content() {
        let palette = test_palette();
        let line = ok_guide(&palette);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[Enter/Esc]"));
        assert!(text.contains("OK"));
    }

    #[test]
    fn test_cancel_guide_content() {
        let palette = test_palette();
        let line = cancel_guide(&palette);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[Esc]"));
        assert!(text.contains("Cancel"));
    }

    #[test]
    fn test_confirm_cancel_guide_uses_palette_positive() {
        let palette = test_palette();
        let line = confirm_cancel_guide(&palette, None);
        // [Y] span は positive 色を使う
        let y_span = line
            .spans
            .iter()
            .find(|s| s.content.contains("[Y]"))
            .unwrap();
        assert_eq!(y_span.style.fg, Some(palette.positive));
    }

    #[test]
    fn test_ok_guide_uses_palette_info() {
        let palette = test_palette();
        let line = ok_guide(&palette);
        let enter_span = line
            .spans
            .iter()
            .find(|s| s.content.contains("[Enter/Esc]"))
            .unwrap();
        assert_eq!(enter_span.style.fg, Some(palette.info));
    }

    #[test]
    fn test_cancel_guide_uses_palette_negative() {
        let palette = test_palette();
        let line = cancel_guide(&palette);
        let esc_span = line
            .spans
            .iter()
            .find(|s| s.content.contains("[Esc]"))
            .unwrap();
        assert_eq!(esc_span.style.fg, Some(palette.negative));
    }
}
