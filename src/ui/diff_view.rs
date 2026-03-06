//! 2カラム diff パネルの描画。
//! パレット経由の色管理 + シンタックスハイライト対応。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use unicode_width::UnicodeWidthStr;

use crate::app::{AppState, DiffMode, Focus};
use crate::diff::engine::{DiffHunk, DiffLine, DiffResult, DiffTag};
use crate::highlight::StyledSegment;
use crate::theme::palette::ensure_contrast;
use crate::theme::TuiPalette;
use crate::ui::metadata;

/// Style にオプショナルな背景色を適用する。
fn style_with_bg(style: Style, bg: Option<Color>) -> Style {
    match bg {
        Some(bg) => style.bg(bg),
        None => style,
    }
}

/// 表示幅ベースでトランケーション or パディングする
fn truncate_or_pad(value: &str, width: usize) -> String {
    let display_width = UnicodeWidthStr::width(value);
    if display_width > width {
        // 表示幅で切る（文字境界を壊さない）
        let target = width.saturating_sub(1); // 「…」分
        let mut current_width = 0;
        let mut end = 0;
        for (i, ch) in value.char_indices() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if current_width + ch_width > target {
                break;
            }
            current_width += ch_width;
            end = i + ch.len_utf8();
        }
        let mut result = value[..end].to_string();
        result.push('…');
        // 残り幅をスペースで埋める（「…」は幅1）
        let used = current_width + 1;
        if used < width {
            result.extend(std::iter::repeat_n(' ', width - used));
        }
        result
    } else {
        // パディング
        let pad = width.saturating_sub(display_width);
        let mut result = value.to_string();
        result.extend(std::iter::repeat_n(' ', pad));
        result
    }
}

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
            Some(n) => format!("{:>5}", n + 1),
            None => "     ".to_string(),
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

    /// diff タグに応じたベース背景色をパレットから取得
    fn base_bg(palette: &TuiPalette, tag: DiffTag) -> Option<Color> {
        match tag {
            DiffTag::Insert => Some(palette.diff_insert_bg),
            DiffTag::Delete => Some(palette.diff_delete_bg),
            DiffTag::Equal => None,
        }
    }

    /// diff タグに応じたプレフィックスのスタイル（パレット版）
    fn prefix_style(palette: &TuiPalette, tag: DiffTag) -> Style {
        match tag {
            DiffTag::Equal => Style::default().fg(palette.gutter_fg),
            DiffTag::Insert => Style::default()
                .fg(palette.diff_insert_fg)
                .add_modifier(Modifier::BOLD),
            DiffTag::Delete => Style::default()
                .fg(palette.diff_delete_fg)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// 背景色の優先度を解決する
    fn resolve_bg(
        palette: &TuiPalette,
        tag: DiffTag,
        is_current_hunk: bool,
        is_focused: bool,
        is_pending: bool,
        is_cursor_line: bool,
    ) -> Option<Color> {
        if is_current_hunk && is_focused {
            if is_pending {
                Some(palette.hunk_pending_bg)
            } else {
                Some(palette.hunk_select_bg)
            }
        } else {
            let base = Self::base_bg(palette, tag);
            if base.is_some() {
                base
            } else if is_cursor_line && is_focused {
                Some(palette.cursor_line_bg)
            } else {
                None
            }
        }
    }

    /// テキスト部分の Span リストを構築する（ハイライト + 検索マッチ対応）
    fn text_spans(state: &AppState, line: &DiffLine, bg: Option<Color>) -> Vec<Span<'static>> {
        let base_spans = if !state.syntax_highlight_enabled {
            vec![Self::plain_text_span(state, line, bg)]
        } else {
            let segments = Self::get_highlight_segments(state, line);
            match segments {
                Some(segs) if !segs.is_empty() => segs
                    .iter()
                    .map(|seg| {
                        let fg_raw = seg.fg.unwrap_or(state.palette.fg);
                        let effective_bg = bg.unwrap_or(state.palette.bg);
                        let fg = ensure_contrast(fg_raw, effective_bg);
                        let style =
                            style_with_bg(Style::default().fg(fg).add_modifier(seg.modifier), bg);
                        Span::styled(seg.text.clone(), style)
                    })
                    .collect(),
                _ => vec![Self::plain_text_span(state, line, bg)],
            }
        };

        // Diff 検索ハイライトを適用
        let query = &state.diff_search_state.query;
        if query.is_empty() {
            base_spans
        } else {
            apply_search_highlight(base_spans, query, state.palette.accent)
        }
    }

    /// プレーンテキスト（ハイライトなし）の Span
    fn plain_text_span(state: &AppState, line: &DiffLine, bg: Option<Color>) -> Span<'static> {
        let fg = match line.tag {
            DiffTag::Equal => state.palette.fg,
            DiffTag::Insert => state.palette.diff_insert_fg,
            DiffTag::Delete => state.palette.diff_delete_fg,
        };
        Span::styled(
            line.value.clone(),
            style_with_bg(Style::default().fg(fg), bg),
        )
    }

    /// ハイライトキャッシュから該当行のセグメントを取得
    fn get_highlight_segments<'b>(
        state: &'b AppState,
        line: &DiffLine,
    ) -> Option<&'b Vec<StyledSegment>> {
        let path = state.selected_path.as_deref()?;
        match line.tag {
            DiffTag::Delete | DiffTag::Equal => {
                let idx = line.old_index?;
                let cached = state.highlight_cache_local.get(path)?;
                cached.get(idx)
            }
            DiffTag::Insert => {
                let idx = line.new_index?;
                let cached = state.highlight_cache_remote.get(path)?;
                cached.get(idx)
            }
        }
    }

    /// diff 行を Line に変換（パレット + ハイライト対応）
    pub fn render_diff_line_highlighted(
        state: &AppState,
        line: &DiffLine,
        is_current_hunk: bool,
        is_focused: bool,
        is_pending: bool,
        is_cursor_line: bool,
    ) -> Line<'static> {
        let p = &state.palette;
        let old_num = Self::format_line_num(line.old_index);
        let new_num = Self::format_line_num(line.new_index);
        let prefix = Self::tag_char(line.tag);
        let num_style = Style::default().fg(p.gutter_fg);
        let prefix_style = Self::prefix_style(p, line.tag);

        let bg = Self::resolve_bg(
            p,
            line.tag,
            is_current_hunk,
            is_focused,
            is_pending,
            is_cursor_line,
        );

        let num_style = style_with_bg(num_style, bg);
        let prefix_style = style_with_bg(prefix_style, bg);

        // インジケータ
        let (indicator_char, indicator_color) = if is_current_hunk && is_focused {
            if is_pending {
                ("⏎ ", Color::Yellow)
            } else {
                ("▶ ", p.accent)
            }
        } else {
            ("  ", Color::Reset)
        };

        let indicator_style = style_with_bg(Style::default().fg(indicator_color), bg);
        let gap_style = bg.map(|b| Style::default().bg(b)).unwrap_or_default();

        let mut spans = vec![
            Span::styled(indicator_char, indicator_style),
            Span::styled(old_num, num_style),
            Span::styled(" ", gap_style),
            Span::styled(new_num, num_style),
            Span::styled(" ", gap_style),
            Span::styled(prefix.to_string(), prefix_style),
            Span::styled(" ", gap_style),
        ];
        spans.extend(Self::text_spans(state, line, bg));

        Line::from(spans)
    }

    /// Side-by-Side 用に diff 行を左右にペアリングする
    pub fn split_for_side_by_side(
        lines: &[DiffLine],
    ) -> Vec<(Option<&DiffLine>, Option<&DiffLine>)> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            match lines[i].tag {
                DiffTag::Equal => {
                    result.push((Some(&lines[i]), Some(&lines[i])));
                    i += 1;
                }
                DiffTag::Delete => {
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
                        let left = if j < delete_count {
                            Some(&lines[delete_start + j])
                        } else {
                            None
                        };
                        let right = if j < insert_count {
                            Some(&lines[delete_end + j])
                        } else {
                            None
                        };
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

    /// Side-by-Side 用の1行を Line に変換（パレット対応）
    #[allow(clippy::too_many_arguments)]
    fn render_side_by_side_line(
        state: &AppState,
        left: Option<&DiffLine>,
        right: Option<&DiffLine>,
        half_width: u16,
        is_current_hunk: bool,
        is_focused: bool,
        is_pending: bool,
        is_cursor_line: bool,
    ) -> Line<'static> {
        let p = &state.palette;
        let content_width = (half_width as usize).saturating_sub(8);

        let hunk_bg = if is_current_hunk && is_focused {
            if is_pending {
                Some(p.hunk_pending_bg)
            } else {
                Some(p.hunk_select_bg)
            }
        } else {
            None
        };
        let cursor_bg = if is_cursor_line && is_focused && hunk_bg.is_none() {
            Some(p.cursor_line_bg)
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
                    let truncated = truncate_or_pad(value, content_width);

                    let base_bg = Self::base_bg(p, line.tag);
                    let bg = hunk_bg.or(base_bg).or(cursor_bg);

                    let fg = match line.tag {
                        DiffTag::Equal => p.fg,
                        DiffTag::Insert => p.diff_insert_fg,
                        DiffTag::Delete => p.diff_delete_fg,
                    };
                    let style = style_with_bg(Style::default().fg(fg), bg);
                    let num_style = style_with_bg(Style::default().fg(p.gutter_fg), bg);
                    let pstyle = style_with_bg(Self::prefix_style(p, line.tag), bg);
                    let gap_style = bg.map(|b| Style::default().bg(b)).unwrap_or_default();

                    vec![
                        Span::styled(num, num_style),
                        Span::styled(prefix.to_string(), pstyle),
                        Span::styled(" ", gap_style),
                        Span::styled(truncated, style),
                    ]
                }
                None => {
                    let bg = hunk_bg.or(cursor_bg);
                    let empty = format!("{:<width$}", "", width = content_width + 7);
                    let empty_style = style_with_bg(Style::default().fg(p.gutter_fg), bg);
                    vec![Span::styled(empty, empty_style)]
                }
            }
        };

        let (indicator_char, indicator_color) = if is_current_hunk && is_focused {
            if is_pending {
                ("⏎", Color::Yellow)
            } else {
                ("▶", p.accent)
            }
        } else {
            (" ", Color::Reset)
        };
        let indicator_bg = hunk_bg.or(cursor_bg);
        let indicator_style = style_with_bg(Style::default().fg(indicator_color), indicator_bg);

        let mut spans = vec![Span::styled(indicator_char, indicator_style)];
        spans.extend(render_half(left));
        spans.push(Span::styled("│", Style::default().fg(p.gutter_fg)));
        spans.extend(render_half(right));

        Line::from(spans)
    }
}

impl<'a> Widget for DiffView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.focus == Focus::DiffView;
        let p = &self.state.palette;

        let border_style = if is_focused {
            Style::default().fg(p.border_focused)
        } else {
            Style::default().fg(p.border_unfocused)
        };

        let title = match &self.state.selected_path {
            Some(path) => format!(" {} ", path),
            None => " Diff ".to_string(),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(Style::default().bg(p.bg));

        let full_inner = block.inner(area);
        block.render(area, buf);

        // メタデータ行を描画し、残りの領域を返す
        let inner = self.render_metadata_line(full_inner, buf);

        match &self.state.current_diff {
            None => {
                let msg = Paragraph::new("  Select a file to view diff");
                msg.render(inner, buf);
            }
            Some(DiffResult::Equal) => {
                self.render_equal(inner, buf, is_focused);
            }
            Some(DiffResult::Binary { left, right }) => {
                self.render_binary(inner, buf, left, right);
            }
            Some(DiffResult::SymlinkDiff {
                left_target,
                right_target,
            }) => {
                self.render_symlink_diff(inner, buf, left_target, right_target);
            }
            Some(DiffResult::Modified {
                hunks: _,
                merge_hunks,
                lines,
                stats,
                ..
            }) => {
                self.render_modified(inner, buf, is_focused, merge_hunks, lines, stats);
            }
        }
    }
}

/// メタデータ行の描画
impl<'a> DiffView<'a> {
    /// 選択中ファイルのメタデータ行を inner 先頭に描画し、残り領域を返す。
    /// ファイル未選択時は inner をそのまま返す。
    fn render_metadata_line(&self, inner: Rect, buf: &mut Buffer) -> Rect {
        let path = match &self.state.selected_path {
            Some(p) => p,
            None => return inner,
        };

        if inner.height < 2 {
            return inner;
        }

        let p = &self.state.palette;
        let local_node = self.state.local_tree.find_node(std::path::Path::new(path));
        let remote_node = self.state.remote_tree.find_node(std::path::Path::new(path));

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
}

/// Binary / SymlinkDiff の描画
impl<'a> DiffView<'a> {
    fn render_binary(
        &self,
        inner: Rect,
        buf: &mut Buffer,
        left: &Option<crate::diff::binary::BinaryInfo>,
        right: &Option<crate::diff::binary::BinaryInfo>,
    ) {
        use crate::diff::binary::format_size;

        let mut lines: Vec<Line<'_>> = Vec::new();
        lines.push(Line::from(Span::styled(
            "  Binary file - content diff not available",
            Style::default().fg(Color::Yellow),
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

    fn render_symlink_diff(
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
}

/// Equal / Modified の描画をメソッド分離（Widget impl 内の行数削減）
impl<'a> DiffView<'a> {
    fn render_equal(&self, inner: Rect, buf: &mut Buffer, is_focused: bool) {
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
            if let Some(content) = self.state.local_cache.get(path) {
                let content_lines: Vec<&str> = content.lines().collect();
                let total_lines = content_lines.len();
                let scroll = self.state.diff_scroll.min(total_lines.saturating_sub(1));
                let cursor = self.state.diff_cursor;
                let highlight_data = if self.state.syntax_highlight_enabled {
                    self.state.highlight_cache_local.get(path)
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
        Self::fill_line_backgrounds(inner, buf, &line_bgs);
    }

    fn render_modified(
        &self,
        inner: Rect,
        buf: &mut Buffer,
        is_focused: bool,
        merge_hunks: &[DiffHunk],
        lines: &[DiffLine],
        stats: &crate::diff::engine::DiffStats,
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

        let mut display_lines: Vec<Line> = match self.state.diff_mode {
            DiffMode::Unified => lines
                .iter()
                .enumerate()
                .skip(scroll)
                .take(visible_height.saturating_sub(1))
                .map(|(line_idx, line)| {
                    let in_current_hunk = current_hunk
                        .map(|h| Self::is_line_in_hunk(line, h))
                        .unwrap_or(false);
                    let is_cursor = line_idx == cursor;
                    let bg = Self::resolve_bg(
                        p,
                        line.tag,
                        in_current_hunk,
                        is_focused,
                        is_pending,
                        is_cursor,
                    );
                    line_bgs.push(bg);
                    Self::render_diff_line_highlighted(
                        self.state,
                        line,
                        in_current_hunk,
                        is_focused,
                        is_pending,
                        is_cursor,
                    )
                })
                .collect(),
            DiffMode::SideBySide => {
                let pairs = Self::split_for_side_by_side(lines);
                let half_width = (inner.width.saturating_sub(1)) / 2;
                pairs
                    .iter()
                    .enumerate()
                    .skip(scroll)
                    .take(visible_height.saturating_sub(1))
                    .map(|(pair_idx, (left, right))| {
                        let in_current_hunk = current_hunk
                            .map(|h| {
                                let left_match =
                                    left.map(|l| Self::is_line_in_hunk(l, h)).unwrap_or(false);
                                let right_match =
                                    right.map(|r| Self::is_line_in_hunk(r, h)).unwrap_or(false);
                                left_match || right_match
                            })
                            .unwrap_or(false);
                        let is_cursor = pair_idx == cursor;
                        // Side-by-Side: hunk/cursor bg のみ行末塗りつぶし対象
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
                        Self::render_side_by_side_line(
                            self.state,
                            *left,
                            *right,
                            half_width,
                            in_current_hunk,
                            is_focused,
                            is_pending,
                            is_cursor,
                        )
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
        Self::fill_line_backgrounds(inner, buf, &line_bgs);
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
}

/// Span リストに対して検索クエリのマッチ部分をハイライトする。
///
/// 各 Span のテキストを case-insensitive で分割し、マッチ部分に
/// 黒文字+アクセント背景のスタイルを適用する。
fn apply_search_highlight<'a>(
    spans: Vec<Span<'a>>,
    query: &str,
    highlight_color: Color,
) -> Vec<Span<'a>> {
    if query.is_empty() {
        return spans;
    }
    let query_lower = query.to_lowercase();
    let highlight_style = Style::default()
        .fg(Color::Black)
        .bg(highlight_color)
        .add_modifier(Modifier::BOLD);

    let mut result = Vec::new();
    for span in spans {
        let text = span.content.as_ref();
        let text_lower = text.to_lowercase();
        let base_style = span.style;

        let mut last = 0;
        for (start, _) in text_lower.match_indices(&query_lower) {
            let end = start + query.len();
            if start > last {
                result.push(Span::styled(text[last..start].to_string(), base_style));
            }
            result.push(Span::styled(text[start..end].to_string(), highlight_style));
            last = end;
        }
        if last < text.len() {
            result.push(Span::styled(text[last..].to_string(), base_style));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::diff::engine::{DiffResult, DiffStats};
    use crate::tree::FileTree;
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
            "develop".to_string(),
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
            merge_hunk_line_indices: vec![],
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
    fn test_insert_line_has_palette_background() {
        let state = make_test_state_with_diff(None);
        let line = DiffLine {
            tag: DiffTag::Insert,
            value: "new line".to_string(),
            old_index: None,
            new_index: Some(0),
        };
        let rendered =
            DiffView::render_diff_line_highlighted(&state, &line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg,
            Some(state.palette.diff_insert_bg),
            "Insert 行にパレットの diff_insert_bg が設定されるべき"
        );
    }

    #[test]
    fn test_delete_line_has_palette_background() {
        let state = make_test_state_with_diff(None);
        let line = DiffLine {
            tag: DiffTag::Delete,
            value: "old line".to_string(),
            old_index: Some(0),
            new_index: None,
        };
        let rendered =
            DiffView::render_diff_line_highlighted(&state, &line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg,
            Some(state.palette.diff_delete_bg),
            "Delete 行にパレットの diff_delete_bg が設定されるべき"
        );
    }

    #[test]
    fn test_equal_line_no_background() {
        let state = make_test_state_with_diff(None);
        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "same line".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };
        let rendered =
            DiffView::render_diff_line_highlighted(&state, &line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg, None,
            "Equal 行には背景色が設定されないべき"
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

    #[test]
    fn test_syntax_highlight_applied_when_cached() {
        use crate::highlight::engine::StyledSegment;

        let mut state = make_test_state_with_diff(None);
        state.selected_path = Some("test.rs".to_string());
        // ハイライトキャッシュに手動でエントリを追加
        let highlight_data = vec![vec![
            StyledSegment {
                text: "fn".to_string(),
                fg: Some(Color::Rgb(180, 142, 173)),
                modifier: Modifier::empty(),
            },
            StyledSegment {
                text: " main".to_string(),
                fg: Some(Color::Rgb(143, 161, 179)),
                modifier: Modifier::empty(),
            },
        ]];
        state
            .highlight_cache_local
            .insert("test.rs".to_string(), highlight_data);

        let line = DiffLine {
            tag: DiffTag::Delete,
            value: "fn main".to_string(),
            old_index: Some(0),
            new_index: None,
        };

        let rendered =
            DiffView::render_diff_line_highlighted(&state, &line, false, false, false, false);
        // ハイライトセグメントが Span に反映されていること
        // 7つの先頭 Span (indicator, old_num, gap, new_num, gap, prefix, gap) + 2セグメント
        assert!(
            rendered.spans.len() >= 9,
            "ハイライト時は複数セグメントの Span が生成されるべき (got {})",
            rendered.spans.len()
        );
        // fn セグメントの fg を確認
        let fn_span = &rendered.spans[7];
        assert_eq!(fn_span.style.fg, Some(Color::Rgb(180, 142, 173)));
    }

    #[test]
    fn test_syntax_highlight_disabled_uses_plain() {
        let mut state = make_test_state_with_diff(None);
        state.syntax_highlight_enabled = false;

        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "hello".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };

        let rendered =
            DiffView::render_diff_line_highlighted(&state, &line, false, false, false, false);
        // ハイライト無効時は 7 + 1 = 8 Span
        assert_eq!(rendered.spans.len(), 8);
    }

    #[test]
    fn test_truncate_or_pad_ascii() {
        // パディング
        let result = truncate_or_pad("hello", 10);
        assert_eq!(result, "hello     ");
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 10);

        // ちょうど
        let result = truncate_or_pad("hello", 5);
        assert_eq!(result, "hello");

        // トランケーション
        let result = truncate_or_pad("hello world", 8);
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 8);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_or_pad_multibyte() {
        // 全角文字（各2幅）のパディング
        let result = truncate_or_pad("あいう", 10);
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 10);
        assert!(result.starts_with("あいう"));

        // 全角文字のトランケーション（幅6 → 幅5に収める）
        let result = truncate_or_pad("あいう", 5);
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 5);
        assert!(result.contains('…'));

        // 混在
        let result = truncate_or_pad("abあい", 10);
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 10);
    }
}
