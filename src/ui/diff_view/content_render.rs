//! 各種 diff コンテンツの描画（equal / modified / binary / symlink / metadata）。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::DiffMode;
use crate::diff::engine::{DiffHunk, DiffLine, DiffStats};
use crate::theme::palette::ensure_contrast;
use crate::ui::metadata;

use super::line_render::{
    is_line_in_hunk, render_diff_line_highlighted, render_side_by_side_line, split_for_side_by_side,
};
use super::search::apply_search_highlight;
use super::style_utils::{resolve_bg, style_with_bg};
use super::three_way_badge::{build_ref_context, side_by_side_line_badge, unified_line_badge};
use super::DiffView;

impl<'a> DiffView<'a> {
    /// 選択中ファイルのメタデータ行を inner 先頭に描画し、残り領域を返す。
    /// ファイル未選択時は inner をそのまま返す。
    pub(super) fn render_metadata_line(&self, inner: Rect, buf: &mut Buffer) -> Rect {
        let path = match &self.state.selected_path {
            Some(p) => p,
            None => return inner,
        };

        if inner.height < 2 {
            return inner;
        }

        let p = &self.state.palette;
        let local_node = self.state.left_tree.find_node(std::path::Path::new(path));
        let remote_node = self.state.right_tree.find_node(std::path::Path::new(path));

        let local_meta = match local_node {
            Some(n) => metadata::format_metadata_line(n.mtime, n.permissions, n.size),
            None => "not found".to_string(),
        };
        let remote_meta = match remote_node {
            Some(n) => metadata::format_metadata_line(n.mtime, n.permissions, n.size),
            None => "not found".to_string(),
        };

        let line = Line::from(vec![
            Span::styled("L: ", Style::default().fg(p.gutter_fg)),
            Span::styled(local_meta, Style::default().fg(p.gutter_fg)),
            Span::styled("  R: ", Style::default().fg(p.gutter_fg)),
            Span::styled(remote_meta, Style::default().fg(p.gutter_fg)),
        ]);

        let meta_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        Paragraph::new(line).render(meta_area, buf);

        // 残りの領域を返す
        Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: inner.height - 1,
        }
    }

    pub(super) fn render_binary(
        &self,
        inner: Rect,
        buf: &mut Buffer,
        left: &Option<crate::diff::binary::BinaryInfo>,
        right: &Option<crate::diff::binary::BinaryInfo>,
    ) {
        use crate::format::format_size;

        let mut lines: Vec<Line<'_>> = Vec::new();
        lines.push(Line::from(Span::styled(
            "  Binary file - content diff not available",
            Style::default().fg(self.state.palette.dialog_accent),
        )));
        lines.push(Line::from(""));

        let half_width = inner.width as usize / 2;
        let label_style = Style::default().fg(Color::DarkGray);
        let value_style = Style::default().fg(Color::White);

        // サイズ行
        let left_size = left
            .as_ref()
            .map_or("(not loaded)".to_string(), |i| format_size(i.size));
        let right_size = right
            .as_ref()
            .map_or("(not loaded)".to_string(), |i| format_size(i.size));
        let left_part = format!("  size: {}", left_size);
        let right_part = format!("  size: {}", right_size);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<width$}", left_part, width = half_width),
                label_style,
            ),
            Span::styled(right_part, label_style),
        ]));

        // SHA-256行
        let left_hash = left
            .as_ref()
            .map_or("(not loaded)".to_string(), |i| i.short_hash());
        let right_hash = right
            .as_ref()
            .map_or("(not loaded)".to_string(), |i| i.short_hash());
        let left_part = format!("  sha256: {}", left_hash);
        let right_part = format!("  sha256: {}", right_hash);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<width$}", left_part, width = half_width),
                value_style,
            ),
            Span::styled(right_part, value_style),
        ]));

        // 比較結果
        if let (Some(l), Some(r)) = (left, right) {
            let cmp = crate::diff::binary::compare(l, r);
            let (msg, color) = match cmp {
                crate::diff::binary::BinaryComparison::Equal => {
                    ("  Status: identical", Color::Green)
                }
                crate::diff::binary::BinaryComparison::Different => {
                    ("  Status: different", Color::Red)
                }
            };
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(msg, Style::default().fg(color))));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }

    pub(super) fn render_symlink_diff(
        &self,
        inner: Rect,
        buf: &mut Buffer,
        left_target: &Option<String>,
        right_target: &Option<String>,
    ) {
        let mut lines: Vec<Line<'_>> = Vec::new();
        lines.push(Line::from(Span::styled(
            "  Symbolic link",
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(""));

        let half_width = inner.width as usize / 2;
        let arrow_style = Style::default().fg(Color::Cyan);

        let left_str = left_target.as_deref().unwrap_or("(not present)");
        let right_str = right_target.as_deref().unwrap_or("(not present)");

        let left_part = format!("  -> {}", left_str);
        let right_part = format!("  -> {}", right_str);

        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<width$}", left_part, width = half_width),
                arrow_style,
            ),
            Span::styled(right_part, arrow_style),
        ]));

        // 比較結果
        let is_same = left_target == right_target;
        let (msg, color) = if is_same {
            ("  Status: identical", Color::Green)
        } else {
            ("  Status: different", Color::Red)
        };
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(msg, Style::default().fg(color))));

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }

    pub(super) fn render_equal(&self, inner: Rect, buf: &mut Buffer, is_focused: bool) {
        let p = &self.state.palette;
        let visible_height = inner.height as usize;
        let mut display_lines: Vec<Line> = Vec::new();
        let mut line_bgs: Vec<Option<Color>> = Vec::new();

        // バナー行
        display_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "✓ Files are identical",
                Style::default()
                    .fg(p.badge_equal)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        line_bgs.push(None);

        if let Some(path) = &self.state.selected_path {
            if let Some(content) = self.state.left_cache.get(path) {
                let content_lines: Vec<&str> = content.lines().collect();
                let total_lines = content_lines.len();
                let scroll = self.state.diff_scroll.min(total_lines.saturating_sub(1));
                let cursor = self.state.diff_cursor;
                let highlight_data = if self.state.syntax_highlight_enabled {
                    self.state.highlight_cache_left.get(path)
                } else {
                    None
                };

                for (i, text) in content_lines
                    .iter()
                    .enumerate()
                    .skip(scroll)
                    .take(visible_height.saturating_sub(2))
                {
                    let line_num = format!("{:>5} ", i + 1);
                    let is_cursor = i == cursor && is_focused;
                    let bg = if is_cursor {
                        Some(p.cursor_line_bg)
                    } else {
                        None
                    };
                    line_bgs.push(bg);
                    let num_style = style_with_bg(Style::default().fg(p.gutter_fg), bg);

                    let mut spans = vec![Span::styled(line_num, num_style)];

                    // ハイライトがあれば適用
                    let mut text_spans: Vec<Span<'_>> =
                        if let Some(hl) = highlight_data.and_then(|h| h.get(i)) {
                            hl.iter()
                                .map(|seg| {
                                    let fg_raw = seg.fg.unwrap_or(p.fg);
                                    let effective_bg = bg.unwrap_or(p.bg);
                                    let fg = ensure_contrast(fg_raw, effective_bg);
                                    let style = style_with_bg(
                                        Style::default().fg(fg).add_modifier(seg.modifier),
                                        bg,
                                    );
                                    Span::styled(seg.text.clone(), style)
                                })
                                .collect()
                        } else {
                            vec![Span::styled(
                                text.to_string(),
                                style_with_bg(Style::default().fg(p.fg), bg),
                            )]
                        };

                    // Diff 検索ハイライト適用
                    let diff_query = &self.state.diff_search_state.query;
                    if !diff_query.is_empty() {
                        text_spans = apply_search_highlight(text_spans, diff_query, p.accent);
                    }
                    spans.extend(text_spans);

                    display_lines.push(Line::from(spans));
                }

                display_lines.push(Line::from(vec![Span::styled(
                    format!(
                        " ={} | {}/{} | identical",
                        total_lines,
                        cursor + 1,
                        total_lines
                    ),
                    Style::default().fg(p.gutter_fg),
                )]));
                line_bgs.push(None);
            }
        }

        let paragraph = Paragraph::new(display_lines);
        paragraph.render(inner, buf);

        // テキスト末尾から行末まで背景色を塗りつぶす
        fill_line_backgrounds(inner, buf, &line_bgs);
    }

    pub(super) fn render_modified(
        &self,
        inner: Rect,
        buf: &mut Buffer,
        is_focused: bool,
        merge_hunks: &[DiffHunk],
        lines: &[DiffLine],
        stats: &DiffStats,
    ) {
        let p = &self.state.palette;
        let visible_height = inner.height as usize;
        let scroll = self.state.diff_scroll.min(lines.len().saturating_sub(1));
        let cursor = self.state.diff_cursor;

        let current_hunk = merge_hunks.get(self.state.hunk_cursor);
        let is_pending = self.state.pending_hunk_merge.is_some();

        let mode_label = match self.state.diff_mode {
            DiffMode::Unified => "unified",
            DiffMode::SideBySide => "side-by-side",
        };

        // 各行の背景色を収集（Paragraph 描画後にバッファの行末まで塗るため）
        let mut line_bgs: Vec<Option<Color>> = Vec::new();

        // 3way reference context（存在する場合のみ）
        let ref_ctx = build_ref_context(self.state);

        let mut display_lines: Vec<Line> = match self.state.diff_mode {
            DiffMode::Unified => lines
                .iter()
                .enumerate()
                .skip(scroll)
                .take(visible_height.saturating_sub(1))
                .map(|(line_idx, line)| {
                    let in_current_hunk = current_hunk
                        .map(|h| is_line_in_hunk(line_idx, h))
                        .unwrap_or(false);
                    let is_cursor = line_idx == cursor;
                    let bg = resolve_bg(
                        p,
                        line.tag,
                        in_current_hunk,
                        is_focused,
                        is_pending,
                        is_cursor,
                    );
                    line_bgs.push(bg);
                    let mut rendered = render_diff_line_highlighted(
                        self.state,
                        line,
                        in_current_hunk,
                        is_focused,
                        is_pending,
                        is_cursor,
                    );
                    // 3way line badge を追加
                    if let Some(ctx) = &ref_ctx {
                        let badge = unified_line_badge(
                            ctx,
                            line.tag,
                            &line.value,
                            line.old_index,
                            line.new_index,
                            &self.state.palette,
                        );
                        if !badge.content.is_empty() {
                            rendered.spans.push(badge);
                        }
                    }
                    rendered
                })
                .collect(),
            DiffMode::SideBySide => {
                let pairs = split_for_side_by_side(lines);
                let half_width = (inner.width.saturating_sub(1)) / 2;
                pairs
                    .iter()
                    .enumerate()
                    .skip(scroll)
                    .take(visible_height.saturating_sub(1))
                    .map(|(pair_idx, (left, right))| {
                        let in_current_hunk = current_hunk
                            .map(|h| {
                                let left_match = left
                                    .map(|(_, idx)| is_line_in_hunk(idx, h))
                                    .unwrap_or(false);
                                let right_match = right
                                    .map(|(_, idx)| is_line_in_hunk(idx, h))
                                    .unwrap_or(false);
                                left_match || right_match
                            })
                            .unwrap_or(false);
                        let is_cursor = pair_idx == cursor;
                        let hunk_bg = if in_current_hunk && is_focused {
                            if is_pending {
                                Some(p.hunk_pending_bg)
                            } else {
                                Some(p.hunk_select_bg)
                            }
                        } else {
                            None
                        };
                        let cursor_bg = if is_cursor && is_focused && hunk_bg.is_none() {
                            Some(p.cursor_line_bg)
                        } else {
                            None
                        };
                        line_bgs.push(hunk_bg.or(cursor_bg));
                        let mut rendered = render_side_by_side_line(
                            self.state,
                            left.map(|(l, _)| l),
                            right.map(|(r, _)| r),
                            half_width,
                            in_current_hunk,
                            is_focused,
                            is_pending,
                            is_cursor,
                        );
                        // 3way line badge を追加
                        if let Some(ctx) = &ref_ctx {
                            let left_val = left.map(|(l, _)| l.value.as_str());
                            let right_val = right.map(|(r, _)| r.value.as_str());
                            let old_idx = left.and_then(|(l, _)| l.old_index);
                            let new_idx = right.and_then(|(r, _)| r.new_index);
                            let badge = side_by_side_line_badge(
                                ctx,
                                left_val,
                                right_val,
                                old_idx,
                                new_idx,
                                &self.state.palette,
                            );
                            if !badge.content.is_empty() {
                                rendered.spans.push(badge);
                            }
                        }
                        rendered
                    })
                    .collect()
            }
        };

        // サマリー行
        let hunk_info = if !merge_hunks.is_empty() {
            format!(
                " | hunk {}/{}",
                self.state.hunk_cursor + 1,
                merge_hunks.len()
            )
        } else {
            String::new()
        };

        let summary = Line::from(vec![Span::styled(
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
            Style::default().fg(p.gutter_fg),
        )]);
        display_lines.push(summary);

        let paragraph = Paragraph::new(display_lines);
        paragraph.render(inner, buf);

        // テキスト末尾から行末まで背景色を塗りつぶす
        fill_line_backgrounds(inner, buf, &line_bgs);
    }
}

/// 各行のテキスト末尾から行末まで背景色を塗りつぶす
fn fill_line_backgrounds(area: Rect, buf: &mut Buffer, line_bgs: &[Option<Color>]) {
    for (row_idx, bg) in line_bgs.iter().enumerate() {
        let Some(bg) = bg else { continue };
        let y = area.y + row_idx as u16;
        if y >= area.y + area.height {
            break;
        }
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(*bg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, Side};
    use crate::diff::engine::DiffResult;
    use crate::tree::FileTree;
    use ratatui::widgets::Widget;
    use std::path::PathBuf;

    fn make_test_state_with_diff(diff: Option<DiffResult>) -> AppState {
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
                    .map(|x| {
                        buf.cell((x, y))
                            .map(|c| c.symbol().to_string())
                            .unwrap_or_default()
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_no_diff_message() {
        let state = make_test_state_with_diff(None);
        let content = render_to_string(&state, 60, 10);
        assert!(
            content.contains("Select a file"),
            "ガイドメッセージが表示されるべき"
        );
    }

    #[test]
    fn test_diff_color_lines() {
        use crate::diff::engine::{DiffLine, DiffStats, DiffTag};

        let lines = vec![
            DiffLine {
                tag: DiffTag::Equal,
                value: "same".to_string(),
                old_index: Some(0),
                new_index: Some(0),
            },
            DiffLine {
                tag: DiffTag::Delete,
                value: "old".to_string(),
                old_index: Some(1),
                new_index: None,
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "new".to_string(),
                old_index: None,
                new_index: Some(1),
            },
            DiffLine {
                tag: DiffTag::Equal,
                value: "end".to_string(),
                old_index: Some(2),
                new_index: Some(2),
            },
        ];

        let diff = DiffResult::Modified {
            hunks: vec![],
            merge_hunks: vec![],
            lines,
            stats: DiffStats {
                insertions: 1,
                deletions: 1,
                equal: 2,
            },
        };

        let state = make_test_state_with_diff(Some(diff));
        let content = render_to_string(&state, 80, 15);

        assert!(content.contains("same"), "Equal行が表示されるべき");
        assert!(content.contains("old"), "Delete行が表示されるべき");
        assert!(content.contains("new"), "Insert行が表示されるべき");
        assert!(content.contains("+1"), "統計が表示されるべき");
    }

    #[test]
    fn test_hunk_highlight_rendering() {
        use crate::app::Focus;
        use crate::diff::engine::compute_diff;

        let old = "aaa\nbbb\nccc\n";
        let new = "aaa\nXXX\nccc\n";
        let diff = compute_diff(old, new);

        let mut state = make_test_state_with_diff(Some(diff));
        state.focus = Focus::DiffView;
        state.hunk_cursor = 0;

        let content = render_to_string(&state, 80, 15);
        assert!(content.contains("aaa"), "コンテキスト行が表示されるべき");
        assert!(content.contains("XXX"), "Insert行が表示されるべき");
        assert!(content.contains("bbb"), "Delete行が表示されるべき");
        assert!(
            content.contains("hunk 1/1"),
            "ハンク情報がサマリーに表示されるべき"
        );
    }

    #[test]
    fn test_hunk_cursor_indicator() {
        use crate::app::Focus;
        use crate::diff::engine::compute_diff;

        let old = "aaa\nbbb\nccc\n";
        let new = "aaa\nXXX\nccc\n";
        let diff = compute_diff(old, new);

        let mut state = make_test_state_with_diff(Some(diff));
        state.focus = Focus::DiffView;
        state.hunk_cursor = 0;

        let content = render_to_string(&state, 80, 15);
        assert!(
            content.contains("▶"),
            "ハンクカーソルインジケータが表示されるべき"
        );
    }

    #[test]
    fn test_binary_diff_display() {
        let state = make_test_state_with_diff(Some(DiffResult::Binary {
            left: Some(crate::diff::binary::BinaryInfo::from_bytes(
                b"hello\x00world",
            )),
            right: Some(crate::diff::binary::BinaryInfo::from_bytes(
                b"different\x00data",
            )),
        }));
        let content = render_to_string(&state, 60, 10);
        assert!(
            content.contains("Binary"),
            "バイナリメッセージが表示されるべき"
        );
    }
}
