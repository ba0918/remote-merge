//! 全体レイアウト定義。
//! ヘッダ、ファイルツリー、diff ビュー、ステータスバーの配置を担当。

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// メインレイアウトの各領域
pub struct AppLayout {
    pub header: Rect,
    pub tree_pane: Rect,
    pub diff_pane: Rect,
    pub status_bar: Rect,
}

impl AppLayout {
    /// ターミナル領域から各ペインの Rect を計算する
    pub fn new(area: Rect) -> Self {
        // 縦分割: ヘッダ(1行) | メイン | ステータスバー(1行)
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // ヘッダ
                Constraint::Min(5),    // メイン領域
                Constraint::Length(1), // ステータスバー
            ])
            .split(area);

        // メイン領域を横分割: ツリー(30%) | diff(70%)
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(vertical[1]);

        Self {
            header: vertical[0],
            tree_pane: horizontal[0],
            diff_pane: horizontal[1],
            status_bar: vertical[2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_splits() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = AppLayout::new(area);

        // ヘッダは1行
        assert_eq!(layout.header.height, 1);
        // ステータスバーは1行
        assert_eq!(layout.status_bar.height, 1);
        // ツリーとdiffの高さは同じ
        assert_eq!(layout.tree_pane.height, layout.diff_pane.height);
        // メイン領域の高さ = 全体 - ヘッダ - ステータスバー
        assert_eq!(layout.tree_pane.height, 38);
    }
}
