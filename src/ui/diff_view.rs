//! 2カラム diff パネルの描画。
//! 追加行（緑）、削除行（赤）、コンテキスト行の色分け表示。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::{AppState, DiffMode, Focus};
use crate::diff::engine::{DiffHunk, DiffLine, DiffResult, DiffTag};

/// diff ビューウィジェット
pub struct DiffView<'a> {
    state: &'a AppState,
}

impl<'a> DiffView<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// 行番号をフォーマット（5桁右寄せ）
    fn format_line_num(num: Option<usize>) -> String {
        match num {
            Some(n) => format!("{:>5}", n + 1), // 1-indexed
            None => "     ".to_string(),
        }
    }

    /// diff タグに応じたスタイルを返す
    fn line_style(tag: DiffTag) -> Style {
        match tag {
            DiffTag::Equal => Style::default().fg(Color::White),
            DiffTag::Insert => Style::default().fg(Color::Green).bg(Color::Rgb(0, 30, 0)),
            DiffTag::Delete => Style::default().fg(Color::Red).bg(Color::Rgb(30, 0, 0)),
        }
    }

    /// diff タグに応じたベース背景色を返す（ハンクハイライトより低優先度）
    fn base_bg(tag: DiffTag) -> Option<Color> {
        match tag {
            DiffTag::Insert => Some(Color::Rgb(0, 30, 0)),
            DiffTag::Delete => Some(Color::Rgb(30, 0, 0)),
            DiffTag::Equal => None,
        }
    }

    /// diff タグのプレフィックス文字
    fn tag_char(tag: DiffTag) -> &'static str {
        match tag {
            DiffTag::Equal => " ",
            DiffTag::Insert => "+",
            DiffTag::Delete => "-",
        }
    }

    /// 行が指定ハンク内に含まれるかチェック
    fn is_line_in_hunk(line: &DiffLine, hunk: &DiffHunk) -> bool {
        hunk.lines.iter().any(|hl| {
            hl.tag == line.tag
                && hl.value == line.value
                && hl.old_index == line.old_index
                && hl.new_index == line.new_index
        })
    }

    /// diff 行を Line に変換（ハンクハイライト付き）
    ///
    /// `is_cursor_line`: この行がカーソルライン（スクロール先頭行）かどうか
    pub fn render_diff_line_with_highlight(
        line: &DiffLine,
        is_current_hunk: bool,
        is_focused: bool,
        is_pending: bool,
        is_cursor_line: bool,
    ) -> Line<'static> {
        let style = Self::line_style(line.tag);
        let old_num = Self::format_line_num(line.old_index);
        let new_num = Self::format_line_num(line.new_index);
        let prefix = Self::tag_char(line.tag);
        let num_style = Style::default().fg(Color::DarkGray);
        let prefix_style = match line.tag {
            DiffTag::Equal => Style::default().fg(Color::DarkGray),
            DiffTag::Insert => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            DiffTag::Delete => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        };

        // 背景色の優先度: ハンクハイライト > diff色(Insert/Delete) > カーソルライン > なし
        let base_bg = Self::base_bg(line.tag);
        let cursor_bg = Color::Rgb(30, 30, 50);

        let bg = if is_current_hunk && is_focused {
            if is_pending {
                Some(Color::Rgb(60, 40, 20)) // オレンジ系: 確定待ち（最優先）
            } else {
                Some(Color::Rgb(40, 40, 60)) // 青系: 通常選択（最優先）
            }
        } else if base_bg.is_some() {
            base_bg // Insert/Delete の背景色
        } else if is_cursor_line && is_focused {
            Some(cursor_bg) // カーソルライン（Equal行のみ適用）
        } else {
            None
        };

        let (style, num_style, prefix_style) = if let Some(bg) = bg {
            (style.bg(bg), num_style.bg(bg), prefix_style.bg(bg))
        } else {
            (style, num_style, prefix_style)
        };

        // カレントハンクのインジケータ
        let (indicator_char, indicator_color) = if is_current_hunk && is_focused {
            if is_pending {
                ("⏎ ", Color::Yellow) // 確定待ち
            } else {
                ("▶ ", Color::Cyan)   // 通常選択
            }
        } else {
            ("  ", Color::Reset)
        };

        let indicator_style = if let Some(bg) = bg {
            Style::default().fg(indicator_color).bg(bg)
        } else {
            Style::default().fg(indicator_color)
        };

        let gap_style = bg.map(|b| Style::default().bg(b)).unwrap_or_default();

        Line::from(vec![
            Span::styled(indicator_char, indicator_style),
            Span::styled(old_num, num_style),
            Span::styled(" ", gap_style),
            Span::styled(new_num, num_style),
            Span::styled(" ", gap_style),
            Span::styled(prefix.to_string(), prefix_style),
            Span::styled(" ", gap_style),
            Span::styled(line.value.clone(), style),
        ])
    }

    /// Side-by-Side 用に diff 行を左右にペアリングする
    ///
    /// 各行が (Option<&DiffLine>, Option<&DiffLine>) のペアになる:
    /// - Equal: (Some(line), Some(line))
    /// - Delete: (Some(line), None) — 次の Insert とペアリングを試みる
    /// - Insert: (None, Some(line))
    pub fn split_for_side_by_side(lines: &[DiffLine]) -> Vec<(Option<&DiffLine>, Option<&DiffLine>)> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            match lines[i].tag {
                DiffTag::Equal => {
                    result.push((Some(&lines[i]), Some(&lines[i])));
                    i += 1;
                }
                DiffTag::Delete => {
                    // Delete/Insert のペアリングを試みる
                    let delete_start = i;
                    while i < lines.len() && lines[i].tag == DiffTag::Delete {
                        i += 1;
                    }
                    let delete_end = i;
                    while i < lines.len() && lines[i].tag == DiffTag::Insert {
                        i += 1;
                    }

                    let delete_count = delete_end - delete_start;
                    let insert_count = i - delete_end;
                    let max_len = delete_count.max(insert_count);
                    for j in 0..max_len {
                        let left = if j < delete_count { Some(&lines[delete_start + j]) } else { None };
                        let right = if j < insert_count { Some(&lines[delete_end + j]) } else { None };
                        result.push((left, right));
                    }
                }
                DiffTag::Insert => {
                    result.push((None, Some(&lines[i])));
                    i += 1;
                }
            }
        }

        result
    }

    /// Side-by-Side 用の1行を Line に変換
    ///
    /// `is_current_hunk`: この行がカレントハンク内かどうか
    /// `is_focused`: DiffView がフォーカスされているか
    /// `is_pending`: 確定待ちマージがあるか
    /// `is_cursor_line`: この行がカーソルラインかどうか
    fn render_side_by_side_line(
        left: Option<&DiffLine>,
        right: Option<&DiffLine>,
        half_width: u16,
        is_current_hunk: bool,
        is_focused: bool,
        is_pending: bool,
        is_cursor_line: bool,
    ) -> Line<'static> {
        let content_width = (half_width as usize).saturating_sub(8); // line_num(5) + tag(1) + spaces(2)

        // ハンクハイライトまたはカーソルラインの背景色を決定
        let hunk_bg = if is_current_hunk && is_focused {
            if is_pending {
                Some(Color::Rgb(60, 40, 20)) // オレンジ系: 確定待ち
            } else {
                Some(Color::Rgb(40, 40, 60)) // 青系: 通常選択
            }
        } else {
            None
        };
        let cursor_bg = if is_cursor_line && is_focused && hunk_bg.is_none() {
            Some(Color::Rgb(30, 30, 50))
        } else {
            None
        };

        let render_half = |line_opt: Option<&DiffLine>| -> Vec<Span<'static>> {
            match line_opt {
                Some(line) => {
                    let num = Self::format_line_num(match line.tag {
                        DiffTag::Delete => line.old_index,
                        DiffTag::Insert => line.new_index,
                        DiffTag::Equal => line.old_index,
                    });
                    let prefix = Self::tag_char(line.tag);
                    let value = &line.value;
                    let truncated = if value.len() > content_width {
                        format!("{}…", &value[..content_width.saturating_sub(1)])
                    } else {
                        format!("{:<width$}", value, width = content_width)
                    };

                    let base_bg = Self::base_bg(line.tag);
                    // 背景色の優先度: ハンクハイライト > diff色 > カーソルライン > なし
                    let bg = hunk_bg.or(base_bg).or(cursor_bg);

                    let style = match bg {
                        Some(bg) => Self::line_style(line.tag).bg(bg),
                        None => Self::line_style(line.tag),
                    };
                    let num_style = match bg {
                        Some(bg) => Style::default().fg(Color::DarkGray).bg(bg),
                        None => Style::default().fg(Color::DarkGray),
                    };
                    let prefix_style = match line.tag {
                        DiffTag::Equal => Style::default().fg(Color::DarkGray),
                        DiffTag::Insert => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                        DiffTag::Delete => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    };
                    let prefix_style = match bg {
                        Some(bg) => prefix_style.bg(bg),
                        None => prefix_style,
                    };
                    let gap_style = bg.map(|b| Style::default().bg(b)).unwrap_or_default();

                    vec![
                        Span::styled(num, num_style),
                        Span::styled(prefix.to_string(), prefix_style),
                        Span::styled(" ", gap_style),
                        Span::styled(truncated, style),
                    ]
                }
                None => {
                    let bg = hunk_bg.or(cursor_bg);
                    let empty = format!("{:<width$}", "", width = content_width + 7);
                    let empty_style = match bg {
                        Some(bg) => Style::default().fg(Color::DarkGray).bg(bg),
                        None => Style::default().fg(Color::DarkGray),
                    };
                    vec![Span::styled(empty, empty_style)]
                }
            }
        };

        // カレントハンクのインジケータ（左端に表示）
        let (indicator_char, indicator_color) = if is_current_hunk && is_focused {
            if is_pending {
                ("⏎", Color::Yellow)
            } else {
                ("▶", Color::Cyan)
            }
        } else {
            (" ", Color::Reset)
        };
        let indicator_bg = hunk_bg.or(cursor_bg);
        let indicator_style = match indicator_bg {
            Some(bg) => Style::default().fg(indicator_color).bg(bg),
            None => Style::default().fg(indicator_color),
        };

        let mut spans = vec![Span::styled(indicator_char, indicator_style)];
        spans.extend(render_half(left));
        spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        spans.extend(render_half(right));

        Line::from(spans)
    }

    /// diff 行を Line に変換（テスト互換用）
    #[allow(dead_code)]
    fn render_diff_line(line: &DiffLine) -> Line<'static> {
        let style = Self::line_style(line.tag);
        let old_num = Self::format_line_num(line.old_index);
        let new_num = Self::format_line_num(line.new_index);
        let prefix = Self::tag_char(line.tag);

        let num_style = Style::default().fg(Color::DarkGray);
        let prefix_style = match line.tag {
            DiffTag::Equal => Style::default().fg(Color::DarkGray),
            DiffTag::Insert => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            DiffTag::Delete => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        };

        Line::from(vec![
            Span::styled(old_num, num_style),
            Span::raw(" "),
            Span::styled(new_num, num_style),
            Span::raw(" "),
            Span::styled(prefix.to_string(), prefix_style),
            Span::raw(" "),
            Span::styled(line.value.clone(), style),
        ])
    }
}

impl<'a> Widget for DiffView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.focus == Focus::DiffView;

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let title = match &self.state.selected_path {
            Some(path) => format!(" {} ", path),
            None => " Diff ".to_string(),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        match &self.state.current_diff {
            None => {
                let msg = Paragraph::new("  Select a file to view diff");
                msg.render(inner, buf);
            }
            Some(DiffResult::Equal) => {
                // Equal: バナー + ファイル内容を表示（読み取り専用）
                let visible_height = inner.height as usize;
                let mut display_lines: Vec<Line> = Vec::new();

                // バナー行
                display_lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("✓ Files are identical", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]));

                // ファイル内容を表示（local_cache から取得）
                if let Some(path) = &self.state.selected_path {
                    if let Some(content) = self.state.local_cache.get(path) {
                        let content_lines: Vec<&str> = content.lines().collect();
                        let total_lines = content_lines.len();
                        let scroll = self.state.diff_scroll.min(total_lines.saturating_sub(1));
                        let cursor = self.state.diff_cursor;

                        for (i, text) in content_lines.iter().enumerate()
                            .skip(scroll)
                            .take(visible_height.saturating_sub(2)) // バナー + サマリー分を引く
                        {
                            let line_num = format!("{:>5} ", i + 1);
                            let is_cursor = i == cursor && is_focused;
                            let bg = if is_cursor {
                                Some(Color::Rgb(30, 30, 50))
                            } else {
                                None
                            };
                            let num_style = match bg {
                                Some(bg) => Style::default().fg(Color::DarkGray).bg(bg),
                                None => Style::default().fg(Color::DarkGray),
                            };
                            let text_style = match bg {
                                Some(bg) => Style::default().fg(Color::White).bg(bg),
                                None => Style::default().fg(Color::White),
                            };
                            display_lines.push(Line::from(vec![
                                Span::styled(line_num, num_style),
                                Span::styled(text.to_string(), text_style),
                            ]));
                        }

                        // サマリー行
                        display_lines.push(Line::from(vec![
                            Span::styled(
                                format!(" ={} | {}/{} | identical", total_lines, cursor + 1, total_lines),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]));
                    }
                }

                let paragraph = Paragraph::new(display_lines);
                paragraph.render(inner, buf);
            }
            Some(DiffResult::Binary) => {
                let msg = Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        "Binary file - diff not available",
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
                msg.render(inner, buf);
            }
            Some(DiffResult::Modified { hunks: _, merge_hunks, lines, stats, .. }) => {
                let visible_height = inner.height as usize;
                let scroll = self.state.diff_scroll.min(lines.len().saturating_sub(1));
                let cursor = self.state.diff_cursor;

                let current_hunk = merge_hunks.get(self.state.hunk_cursor);
                let is_pending = self.state.pending_hunk_merge.is_some();

                let mode_label = match self.state.diff_mode {
                    DiffMode::Unified => "unified",
                    DiffMode::SideBySide => "side-by-side",
                };

                let mut display_lines: Vec<Line> = match self.state.diff_mode {
                    DiffMode::Unified => {
                        lines
                            .iter()
                            .enumerate()
                            .skip(scroll)
                            .take(visible_height.saturating_sub(1))
                            .map(|(line_idx, line)| {
                                let in_current_hunk = current_hunk
                                    .map(|h| Self::is_line_in_hunk(line, h))
                                    .unwrap_or(false);
                                let is_cursor = line_idx == cursor;
                                Self::render_diff_line_with_highlight(line, in_current_hunk, is_focused, is_pending, is_cursor)
                            })
                            .collect()
                    }
                    DiffMode::SideBySide => {
                        let pairs = Self::split_for_side_by_side(lines);
                        let half_width = (inner.width.saturating_sub(1)) / 2; // インジケータ分を引く
                        pairs
                            .iter()
                            .enumerate()
                            .skip(scroll)
                            .take(visible_height.saturating_sub(1))
                            .map(|(pair_idx, (left, right))| {
                                // ペア内のいずれかの行がカレントハンクに含まれるか
                                let in_current_hunk = current_hunk
                                    .map(|h| {
                                        let left_match = left
                                            .map(|l| Self::is_line_in_hunk(l, h))
                                            .unwrap_or(false);
                                        let right_match = right
                                            .map(|r| Self::is_line_in_hunk(r, h))
                                            .unwrap_or(false);
                                        left_match || right_match
                                    })
                                    .unwrap_or(false);
                                let is_cursor = pair_idx == cursor;
                                Self::render_side_by_side_line(
                                    *left, *right, half_width,
                                    in_current_hunk, is_focused, is_pending, is_cursor,
                                )
                            })
                            .collect()
                    }
                };

                // サマリー行（ハンク情報付き）
                let hunk_info = if !merge_hunks.is_empty() {
                    format!(
                        " | hunk {}/{}",
                        self.state.hunk_cursor + 1,
                        merge_hunks.len()
                    )
                } else {
                    String::new()
                };

                let summary = Line::from(vec![
                    Span::styled(
                        format!(
                            " +{} -{} ={} | {}/{}{} | {}",
                            stats.insertions,
                            stats.deletions,
                            stats.equal,
                            cursor + 1,
                            lines.len(),
                            hunk_info,
                            mode_label,
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                display_lines.push(summary);

                let paragraph = Paragraph::new(display_lines);
                paragraph.render(inner, buf);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::diff::engine::{DiffStats, DiffResult};
    use crate::tree::FileTree;
    use std::path::PathBuf;

    fn make_test_state_with_diff(diff: Option<DiffResult>) -> AppState {
        let mut state = AppState::new(
            FileTree { root: PathBuf::from("/test"), nodes: vec![] },
            FileTree { root: PathBuf::from("/test"), nodes: vec![] },
            "develop".to_string(),
        );
        state.current_diff = diff;
        state.selected_path = Some("test.txt".to_string());
        state
    }

    fn render_to_string(state: &AppState, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        let widget = DiffView::new(state);
        widget.render(area, &mut buf);

        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf.cell((x, y)).map(|c| c.symbol().to_string()).unwrap_or_default())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_no_diff_message() {
        let state = make_test_state_with_diff(None);
        let content = render_to_string(&state, 60, 10);
        assert!(content.contains("Select a file"), "ガイドメッセージが表示されるべき");
    }

    #[test]
    fn test_diff_color_lines() {
        let lines = vec![
            DiffLine { tag: DiffTag::Equal,  value: "same".to_string(), old_index: Some(0), new_index: Some(0) },
            DiffLine { tag: DiffTag::Delete, value: "old".to_string(),  old_index: Some(1), new_index: None },
            DiffLine { tag: DiffTag::Insert, value: "new".to_string(),  old_index: None,    new_index: Some(1) },
            DiffLine { tag: DiffTag::Equal,  value: "end".to_string(),  old_index: Some(2), new_index: Some(2) },
        ];

        let diff = DiffResult::Modified {
            hunks: vec![],
            merge_hunks: vec![],
            lines,
            stats: DiffStats { insertions: 1, deletions: 1, equal: 2 },
            merge_hunk_line_indices: vec![],
        };

        let state = make_test_state_with_diff(Some(diff));
        let content = render_to_string(&state, 80, 15);

        // 行内容が描画されていること
        assert!(content.contains("same"), "Equal行が表示されるべき");
        assert!(content.contains("old"), "Delete行が表示されるべき");
        assert!(content.contains("new"), "Insert行が表示されるべき");
        assert!(content.contains("+1"), "統計が表示されるべき");
    }

    #[test]
    fn test_hunk_highlight_rendering() {
        use crate::diff::engine::compute_diff;

        let old = "aaa\nbbb\nccc\n";
        let new = "aaa\nXXX\nccc\n";
        let diff = compute_diff(old, new);

        let mut state = make_test_state_with_diff(Some(diff));
        state.focus = Focus::DiffView; // フォーカス中
        state.hunk_cursor = 0;

        let content = render_to_string(&state, 80, 15);
        // ハンクハイライトが有効で、コンテンツが表示されること
        assert!(content.contains("aaa"), "コンテキスト行が表示されるべき");
        assert!(content.contains("XXX"), "Insert行が表示されるべき");
        assert!(content.contains("bbb"), "Delete行が表示されるべき");
        // ハンク情報がサマリーに含まれること
        assert!(content.contains("hunk 1/1"), "ハンク情報がサマリーに表示されるべき");
    }

    #[test]
    fn test_hunk_cursor_indicator() {
        use crate::diff::engine::compute_diff;

        let old = "aaa\nbbb\nccc\n";
        let new = "aaa\nXXX\nccc\n";
        let diff = compute_diff(old, new);

        let mut state = make_test_state_with_diff(Some(diff));
        state.focus = Focus::DiffView;
        state.hunk_cursor = 0;

        let content = render_to_string(&state, 80, 15);
        // カーソルインジケータ ▶ が描画されていること
        assert!(content.contains("▶"), "ハンクカーソルインジケータが表示されるべき");
    }

    #[test]
    fn test_insert_line_has_green_background() {
        let line = DiffLine {
            tag: DiffTag::Insert,
            value: "new line".to_string(),
            old_index: None,
            new_index: Some(0),
        };
        let rendered = DiffView::render_diff_line_with_highlight(&line, false, false, false, false);
        // Insert 行のスタイルに bg が設定されていることを確認
        // rendered の最後の Span (value) のスタイルをチェック
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg,
            Some(Color::Rgb(0, 30, 0)),
            "Insert 行に緑の背景色が設定されるべき"
        );
    }

    #[test]
    fn test_delete_line_has_red_background() {
        let line = DiffLine {
            tag: DiffTag::Delete,
            value: "old line".to_string(),
            old_index: Some(0),
            new_index: None,
        };
        let rendered = DiffView::render_diff_line_with_highlight(&line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg,
            Some(Color::Rgb(30, 0, 0)),
            "Delete 行に赤の背景色が設定されるべき"
        );
    }

    #[test]
    fn test_equal_line_no_background() {
        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "same line".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };
        let rendered = DiffView::render_diff_line_with_highlight(&line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg,
            None,
            "Equal 行には背景色が設定されないべき"
        );
    }

    #[test]
    fn test_binary_diff_display() {
        let state = make_test_state_with_diff(Some(DiffResult::Binary));
        let content = render_to_string(&state, 60, 10);
        assert!(content.contains("Binary"), "バイナリメッセージが表示されるべき");
    }
}
