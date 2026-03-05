//! モーダルダイアログ（確認ダイアログ + サーバ選択メニュー等）。

mod batch_confirm;
mod confirm;
mod filter_panel;
mod help;
mod hunk_preview;
mod server_menu;

pub use batch_confirm::{BatchConfirmDialog, BatchConfirmDialogWidget};
pub use confirm::{ConfirmDialog, ConfirmDialogWidget};
pub use filter_panel::{FilterPanel, FilterPanelWidget};
pub use help::{HelpOverlay, HelpOverlayWidget, HelpSection};
pub use hunk_preview::{HunkMergePreview, HunkMergePreviewWidget};
pub use server_menu::{ServerMenu, ServerMenuWidget};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
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
    /// サーバ選択メニュー
    ServerSelect(ServerMenu),
    /// フィルターパネル
    Filter(FilterPanel),
    /// ハンクマージプレビュー
    HunkMergePreview(HunkMergePreview),
    /// ヘルプオーバーレイ
    Help(HelpOverlay),
    /// 書き込み確認ダイアログ（w キー）
    WriteConfirmation,
    /// 未保存変更確認ダイアログ（q キー時）
    UnsavedChanges,
}

/// 中央にモーダルエリアを計算する
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// ダイアログ共通フレームを描画し、内部領域を返す。
pub fn render_dialog_frame(
    title: &str,
    border_color: Color,
    width: u16,
    height: u16,
    area: Rect,
    buf: &mut Buffer,
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
        );
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
}
