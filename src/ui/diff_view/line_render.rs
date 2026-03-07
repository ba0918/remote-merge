//! Diff 行のレンダリング（unified / side-by-side）。

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::app::AppState;
use crate::diff::engine::{DiffHunk, DiffLine, DiffTag};
use crate::highlight::StyledSegment;
use crate::theme::palette::ensure_contrast;

use super::search::apply_search_highlight;
use super::style_utils::{
    base_bg, format_line_num, prefix_style, resolve_bg, style_with_bg, tag_char, truncate_or_pad,
};

/// 行が指定ハンク内に含まれるかチェック
pub fn is_line_in_hunk(line: &DiffLine, hunk: &DiffHunk) -> bool {
    hunk.lines.iter().any(|hl| {
        hl.tag == line.tag
            && hl.value == line.value
            && hl.old_index == line.old_index
            && hl.new_index == line.new_index
    })
}

/// テキスト部分の Span リストを構築する（ハイライト + 検索マッチ対応）
fn text_spans(state: &AppState, line: &DiffLine, bg: Option<Color>) -> Vec<Span<'static>> {
    let base_spans = if !state.syntax_highlight_enabled {
        vec![plain_text_span(state, line, bg)]
    } else {
        let segments = get_highlight_segments(state, line);
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
            _ => vec![plain_text_span(state, line, bg)],
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
            let cached = state.highlight_cache_left.get(path)?;
            cached.get(idx)
        }
        DiffTag::Insert => {
            let idx = line.new_index?;
            let cached = state.highlight_cache_right.get(path)?;
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
    let old_num = format_line_num(line.old_index);
    let new_num = format_line_num(line.new_index);
    let prefix = tag_char(line.tag);
    let num_style = Style::default().fg(p.gutter_fg);
    let prefix_style = prefix_style(p, line.tag);

    let bg = resolve_bg(
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
    spans.extend(text_spans(state, line, bg));

    Line::from(spans)
}

/// Side-by-Side 用に diff 行を左右にペアリングする
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
pub fn render_side_by_side_line(
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
                let num = format_line_num(match line.tag {
                    DiffTag::Delete => line.old_index,
                    DiffTag::Insert => line.new_index,
                    DiffTag::Equal => line.old_index,
                });
                let prefix = tag_char(line.tag);
                let value = &line.value;
                let truncated = truncate_or_pad(value, content_width);

                let base_bg = base_bg(p, line.tag);
                let bg = hunk_bg.or(base_bg).or(cursor_bg);

                let fg = match line.tag {
                    DiffTag::Equal => p.fg,
                    DiffTag::Insert => p.diff_insert_fg,
                    DiffTag::Delete => p.diff_delete_fg,
                };
                let style = style_with_bg(Style::default().fg(fg), bg);
                let num_style = style_with_bg(Style::default().fg(p.gutter_fg), bg);
                let pstyle = style_with_bg(prefix_style(p, line.tag), bg);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Side;
    use crate::tree::FileTree;
    use std::path::PathBuf;

    fn make_test_state() -> AppState {
        AppState::new(
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
        )
    }

    #[test]
    fn test_insert_line_has_palette_background() {
        let state = make_test_state();
        let line = DiffLine {
            tag: DiffTag::Insert,
            value: "new line".to_string(),
            old_index: None,
            new_index: Some(0),
        };
        let rendered = render_diff_line_highlighted(&state, &line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg,
            Some(state.palette.diff_insert_bg),
            "Insert 行にパレットの diff_insert_bg が設定されるべき"
        );
    }

    #[test]
    fn test_delete_line_has_palette_background() {
        let state = make_test_state();
        let line = DiffLine {
            tag: DiffTag::Delete,
            value: "old line".to_string(),
            old_index: Some(0),
            new_index: None,
        };
        let rendered = render_diff_line_highlighted(&state, &line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg,
            Some(state.palette.diff_delete_bg),
            "Delete 行にパレットの diff_delete_bg が設定されるべき"
        );
    }

    #[test]
    fn test_equal_line_no_background() {
        let state = make_test_state();
        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "same line".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };
        let rendered = render_diff_line_highlighted(&state, &line, false, false, false, false);
        let value_span = rendered.spans.last().unwrap();
        assert_eq!(
            value_span.style.bg, None,
            "Equal 行には背景色が設定されないべき"
        );
    }

    #[test]
    fn test_syntax_highlight_applied_when_cached() {
        use crate::highlight::engine::StyledSegment;
        use ratatui::style::{Color, Modifier};

        let mut state = make_test_state();
        state.selected_path = Some("test.rs".to_string());
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
            .highlight_cache_left
            .insert("test.rs".to_string(), highlight_data);

        let line = DiffLine {
            tag: DiffTag::Delete,
            value: "fn main".to_string(),
            old_index: Some(0),
            new_index: None,
        };

        let rendered = render_diff_line_highlighted(&state, &line, false, false, false, false);
        assert!(
            rendered.spans.len() >= 9,
            "ハイライト時は複数セグメントの Span が生成されるべき (got {})",
            rendered.spans.len()
        );
        let fn_span = &rendered.spans[7];
        assert_eq!(fn_span.style.fg, Some(Color::Rgb(180, 142, 173)));
    }

    #[test]
    fn test_syntax_highlight_disabled_uses_plain() {
        let mut state = make_test_state();
        state.syntax_highlight_enabled = false;

        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "hello".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };

        let rendered = render_diff_line_highlighted(&state, &line, false, false, false, false);
        assert_eq!(rendered.spans.len(), 8);
    }

    #[test]
    fn test_split_for_side_by_side_equal() {
        let lines = vec![DiffLine {
            tag: DiffTag::Equal,
            value: "same".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        }];
        let pairs = split_for_side_by_side(&lines);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].0.is_some());
        assert!(pairs[0].1.is_some());
    }

    #[test]
    fn test_split_for_side_by_side_delete_insert() {
        let lines = vec![
            DiffLine {
                tag: DiffTag::Delete,
                value: "old".to_string(),
                old_index: Some(0),
                new_index: None,
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "new".to_string(),
                old_index: None,
                new_index: Some(0),
            },
        ];
        let pairs = split_for_side_by_side(&lines);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0.unwrap().value, "old");
        assert_eq!(pairs[0].1.unwrap().value, "new");
    }
}
