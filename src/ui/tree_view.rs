//! ファイルツリーパネルの描画。
//! インデント、開閉マーカー、差分バッジ、スクロール対応。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::{AppState, Badge, Focus};

/// ファイルツリーウィジェット
pub struct TreeView<'a> {
    state: &'a AppState,
}

impl<'a> TreeView<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// バッジの色を返す
    fn badge_style(badge: Badge) -> Style {
        match badge {
            Badge::Modified => Style::default().fg(Color::Yellow),
            Badge::Equal => Style::default().fg(Color::Green),
            Badge::LeftOnly => Style::default().fg(Color::Cyan),
            Badge::RightOnly => Style::default().fg(Color::Magenta),
            Badge::Unchecked => Style::default().fg(Color::DarkGray),
            Badge::Loading => Style::default().fg(Color::Blue),
            Badge::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
    }
}

impl<'a> Widget for TreeView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.focus == Focus::FileTree;

        let p = &self.state.palette;
        let border_style = if is_focused {
            Style::default().fg(p.border_focused)
        } else {
            Style::default().fg(p.border_unfocused)
        };

        let block = Block::default()
            .title(" Files ")
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(Style::default().bg(p.bg));

        let inner = block.inner(area);
        block.render(area, buf);

        if self.state.flat_nodes.is_empty() {
            let empty = Paragraph::new("  (no files)");
            empty.render(inner, buf);
            return;
        }

        // スクロールオフセットは AppState 側で管理（ensure_tree_cursor_visible）
        let visible_height = inner.height as usize;
        let cursor = self.state.tree_cursor;
        let scroll_offset = self.state.tree_scroll;

        let lines: Vec<Line> = self
            .state
            .flat_nodes
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|(i, node)| {
                let indent = "  ".repeat(node.depth);
                let marker = if node.is_dir {
                    if node.expanded {
                        "▼ "
                    } else {
                        "▶ "
                    }
                } else {
                    "  "
                };

                let icon = if node.is_symlink {
                    "@ "
                } else if node.is_dir {
                    "/ "
                } else {
                    "  "
                };

                // ref 差分のみの場合はバッジを DarkGray の [M] に変更
                let ref_badge_opt = if self.state.has_reference() {
                    self.state.compute_ref_badge(&node.path, node.is_dir)
                } else {
                    None
                };
                let (badge_text, badge_style) = if node.badge == Badge::Equal
                    && ref_badge_opt.as_ref().is_some_and(|b| {
                        !matches!(b, crate::app::three_way::ThreeWayFileBadge::AllEqual)
                    }) {
                    // left/right は Equal だが ref に差分あり → DarkGray の [M]
                    ("[M]", Style::default().fg(Color::DarkGray))
                } else {
                    (node.badge.label(), Self::badge_style(node.badge))
                };

                let p = &self.state.palette;
                let is_selected = i == cursor;
                let name_style = if is_selected {
                    Style::default()
                        .fg(p.fg)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else if node.is_dir {
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(p.fg)
                };

                // 検索一致部分のハイライト
                let name_spans = build_name_spans(
                    &node.name,
                    &self.state.search_state.query,
                    name_style,
                    p.accent,
                );

                let mut spans = vec![Span::raw(indent), Span::raw(marker), Span::raw(icon)];
                spans.extend(name_spans);
                spans.push(Span::raw(" "));
                spans.push(Span::styled(badge_text, badge_style));
                if node.is_symlink {
                    spans.push(Span::styled(" [L]", Style::default().fg(Color::Cyan)));
                }

                // 3way reference バッジ（reference サーバが設定されている場合のみ）
                if let Some(ref_badge) = &ref_badge_opt {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(ref_badge.label(), ref_badge.style()));
                }

                Line::from(spans)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

/// ノード名を検索クエリでスプリットし、一致部分をハイライトした Span リストを生成する。
///
/// case-insensitive で最初の一致のみハイライト。クエリが空なら通常表示。
fn build_name_spans<'a>(
    name: &'a str,
    query: &str,
    base_style: Style,
    highlight_color: Color,
) -> Vec<Span<'a>> {
    if query.is_empty() {
        return vec![Span::styled(name.to_string(), base_style)];
    }

    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();

    if let Some(start) = name_lower.find(&query_lower) {
        let end = start + query.len();
        let highlight_style = Style::default()
            .fg(Color::Black)
            .bg(highlight_color)
            .add_modifier(Modifier::BOLD);

        let mut spans = Vec::with_capacity(3);
        if start > 0 {
            spans.push(Span::styled(name[..start].to_string(), base_style));
        }
        spans.push(Span::styled(name[start..end].to_string(), highlight_style));
        if end < name.len() {
            spans.push(Span::styled(name[end..].to_string(), base_style));
        }
        spans
    } else {
        vec![Span::styled(name.to_string(), base_style)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::FlatNode;
    use crate::app::Side;
    use crate::tree::FileTree;
    use std::path::PathBuf;

    fn make_test_state(flat_nodes: Vec<FlatNode>) -> AppState {
        let mut state = AppState::new(
            FileTree {
                root: PathBuf::from("/test"),
                nodes: vec![],
            },
            FileTree {
                root: PathBuf::from("/test"),
                nodes: vec![],
            },
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.flat_nodes = flat_nodes;
        state
    }

    #[test]
    fn test_tree_render_with_badges() {
        let state = make_test_state(vec![
            FlatNode {
                path: "src".to_string(),
                name: "src".to_string(),
                depth: 0,
                is_dir: true,
                is_symlink: false,
                expanded: true,
                badge: Badge::Unchecked,
            },
            FlatNode {
                path: "src/main.rs".to_string(),
                name: "main.rs".to_string(),
                depth: 1,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::Modified,
            },
        ]);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        let widget = TreeView::new(&state);
        widget.render(area, &mut buf);

        // バッファに描画されていることを確認（パニックしなければOK）
        // 具体的な内容はバッファ内の文字列で検証
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

        assert!(content.contains("Files"), "タイトルが表示されるべき");
    }

    #[test]
    fn test_scroll_position() {
        // 多数のノードでスクロールが動作すること
        let mut nodes = Vec::new();
        for i in 0..50 {
            nodes.push(FlatNode {
                path: format!("file{}.txt", i),
                name: format!("file{}.txt", i),
                depth: 0,
                is_dir: false,
                is_symlink: false,
                expanded: false,
                badge: Badge::Unchecked,
            });
        }
        let mut state = make_test_state(nodes);
        state.tree_cursor = 40; // 下の方にカーソル

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        let widget = TreeView::new(&state);
        widget.render(area, &mut buf);
        // パニックしなければOK
    }

    #[test]
    fn test_empty_tree() {
        let state = make_test_state(vec![]);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        let widget = TreeView::new(&state);
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

        assert!(
            content.contains("no files"),
            "空ツリーメッセージが表示されるべき"
        );
    }
}
