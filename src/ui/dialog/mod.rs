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

pub use batch_confirm::{BatchConfirmDialog, BatchConfirmDialogWidget};
pub use confirm::{ConfirmDialog, ConfirmDialogWidget};
pub use filter_panel::{FilterPanel, FilterPanelWidget};
pub use help::{HelpOverlay, HelpOverlayWidget, HelpSection};
pub use hunk_preview::{HunkMergePreview, HunkMergePreviewWidget};
pub use mtime_warning::{MtimeWarningDialog, MtimeWarningDialogWidget, MtimeWarningMergeContext};
pub use pair_server_menu::{Column, PairServerMenu, PairServerMenuWidget};
pub use server_menu::{ServerMenu, ServerMenuWidget};
pub use three_way_summary::ThreeWaySummaryWidget;

use crate::app::three_way_summary::ThreeWaySummaryPanel;
use crate::theme::palette::TuiPalette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};

/// アプリのダイアログ状態
#[derive(Debug, Clone, Default)]
pub enum DialogState {
    /// ダイアログなし
    #[default]
    None,
    /// マージ確認ダイアログ
    Confirm(ConfirmDialog),
    /// バッチマージ確認ダイアログ（ディレクトリ選択時）
    BatchConfirm(BatchConfirmDialog),
    /// サーバ選択メニュー（右側のみ切り替え、後方互換）
    ServerSelect(ServerMenu),
    /// ペアサーバ選択メニュー（LEFT/RIGHT 両方選択可能、3way diff 用）
    PairServerSelect(PairServerMenu),
    /// フィルターパネル
    Filter(FilterPanel),
    /// ハンクマージプレビュー
    HunkMergePreview(HunkMergePreview),
    /// ヘルプオーバーレイ
    Help(HelpOverlay),
    /// 情報ダイアログ（メッセージ表示のみ、Esc/Enter で閉じる）
    Info(String),
    /// プログレスダイアログ（走査・マージ進捗表示）
    Progress(ProgressDialog),
    /// 書き込み確認ダイアログ（w キー）
    WriteConfirmation,
    /// 未保存変更確認ダイアログ（q キー時）
    UnsavedChanges,
    /// mtime 衝突警告ダイアログ（楽観的ロック）
    MtimeWarning(MtimeWarningDialog),
    /// 3way サマリーパネル（W キー）
    ThreeWaySummary(ThreeWaySummaryPanel),
}

/// プログレスダイアログのフェーズ（サービス層はフェーズだけ設定し、UI層が表示テキストを生成）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressPhase {
    /// ディレクトリ走査中（ファイル発見フェーズ）
    Scanning,
    /// ファイルコンテンツ読み込み中
    LoadingFiles,
    /// リモートファイル読み込み中
    LoadingRemote,
    /// マージ実行中
    Merging,
}

impl ProgressPhase {
    /// UI 表示用のタイトルテキストを生成する
    pub fn title(&self) -> &'static str {
        match self {
            ProgressPhase::Scanning => "Scanning",
            ProgressPhase::LoadingFiles => "Loading files",
            ProgressPhase::LoadingRemote => "Loading remote files",
            ProgressPhase::Merging => "Merging",
        }
    }

    /// プログレスバーの不定形式テキスト（total 不明時）
    pub fn indeterminate_text(&self, current: usize) -> String {
        match self {
            ProgressPhase::Scanning => format!("Discovering files... {} found", current),
            _ => format!("Processing... {}", current),
        }
    }
}

/// プログレスダイアログの状態
#[derive(Debug, Clone)]
pub struct ProgressDialog {
    /// 進捗フェーズ
    pub phase: ProgressPhase,
    /// 走査対象のコンテキスト（例: ディレクトリパス）
    pub context: String,
    /// 現在の進捗値
    pub current: usize,
    /// 全体の件数（不明な場合は None）
    pub total: Option<usize>,
    /// 現在処理中のパス（表示用）
    pub current_path: Option<String>,
    /// Esc でキャンセル可能か
    pub cancelable: bool,
}

impl ProgressDialog {
    /// 新しいプログレスダイアログを作成する
    pub fn new(phase: ProgressPhase, context: impl Into<String>, cancelable: bool) -> Self {
        Self {
            phase,
            context: context.into(),
            current: 0,
            total: None,
            current_path: None,
            cancelable,
        }
    }

    /// UI 表示用のタイトルを生成する（フェーズ + コンテキスト）
    pub fn display_title(&self) -> String {
        if self.context.is_empty() {
            self.phase.title().to_string()
        } else {
            format!("{} {}", self.phase.title(), self.context)
        }
    }
}

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
