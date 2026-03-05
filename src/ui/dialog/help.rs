//! ヘルプオーバーレイ。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::render_dialog_frame;

/// ヘルプオーバーレイのセクション
#[derive(Debug, Clone)]
pub struct HelpSection {
    pub title: String,
    pub bindings: Vec<(String, String)>, // (キー, 説明)
}

/// ヘルプオーバーレイの状態
#[derive(Debug, Clone)]
pub struct HelpOverlay {
    pub sections: Vec<HelpSection>,
}

impl Default for HelpOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpOverlay {
    pub fn new() -> Self {
        Self {
            sections: vec![
                HelpSection {
                    title: "File Tree".to_string(),
                    bindings: vec![
                        ("j/↓".to_string(), "カーソル下移動".to_string()),
                        ("k/↑".to_string(), "カーソル上移動".to_string()),
                        ("Enter/l/→".to_string(), "展開 / ファイル選択".to_string()),
                        ("h/←".to_string(), "折りたたみ".to_string()),
                        (
                            "L (Shift)".to_string(),
                            "remote → local マージ (ディレクトリ対応)".to_string(),
                        ),
                        (
                            "R (Shift)".to_string(),
                            "local → remote マージ (ディレクトリ対応)".to_string(),
                        ),
                        (
                            "F (Shift)".to_string(),
                            "変更ファイルのみ表示 (全走査)".to_string(),
                        ),
                        (
                            "r".to_string(),
                            "リフレッシュ / キャッシュクリア".to_string(),
                        ),
                        ("f".to_string(), "フィルターパネル".to_string()),
                        ("s".to_string(), "サーバ選択".to_string()),
                    ],
                },
                HelpSection {
                    title: "Diff View".to_string(),
                    bindings: vec![
                        ("j/k/↑/↓".to_string(), "1行スクロール".to_string()),
                        ("n".to_string(), "次のハンクへジャンプ".to_string()),
                        ("N".to_string(), "前のハンクへジャンプ".to_string()),
                        ("PageDown".to_string(), "ページ下スクロール".to_string()),
                        ("PageUp".to_string(), "ページ上スクロール".to_string()),
                        ("Home".to_string(), "先頭へ".to_string()),
                        ("End".to_string(), "末尾へ".to_string()),
                        (
                            "→/l".to_string(),
                            "ハンク: remote → local 即時適用".to_string(),
                        ),
                        (
                            "←/h".to_string(),
                            "ハンク: local → remote 即時適用".to_string(),
                        ),
                        ("w".to_string(), "変更をファイルに書き込み".to_string()),
                        ("u".to_string(), "最後の操作を undo".to_string()),
                        ("U".to_string(), "全操作を undo".to_string()),
                        ("d".to_string(), "Unified ↔ Side-by-Side 切替".to_string()),
                        ("c".to_string(), "SSH 再接続".to_string()),
                    ],
                },
                HelpSection {
                    title: "Global".to_string(),
                    bindings: vec![
                        ("Tab".to_string(), "フォーカス切替".to_string()),
                        ("?".to_string(), "ヘルプ表示/閉じる".to_string()),
                        ("q".to_string(), "終了".to_string()),
                    ],
                },
            ],
        }
    }
}

/// ヘルプオーバーレイウィジェット
pub struct HelpOverlayWidget<'a> {
    help: &'a HelpOverlay,
}

impl<'a> HelpOverlayWidget<'a> {
    pub fn new(help: &'a HelpOverlay) -> Self {
        Self { help }
    }
}

impl<'a> Widget for HelpOverlayWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let total_lines: usize = self
            .help
            .sections
            .iter()
            .map(|s| s.bindings.len() + 2)
            .sum();

        let width = area.width.min(60);
        let height = ((total_lines as u16) + 4).min(area.height);
        let inner =
            render_dialog_frame(" Help (? to close) ", Color::Cyan, width, height, area, buf);

        let mut lines: Vec<Line> = Vec::new();

        for section in &self.help.sections {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("── {} ──", section.title),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            for (key, desc) in &section.bindings {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<16}", key), Style::default().fg(Color::Cyan)),
                    Span::styled(desc.clone(), Style::default().fg(Color::White)),
                ]));
            }

            lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}
