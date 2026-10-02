//! Diff 行のレンダリング（unified / side-by-side）。

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::app::AppState;
use crate::diff::engine::{DiffHunk, DiffLine, DiffTag};
use crate::highlight::StyledSegment;
use crate::theme::palette::ensure_contrast;

/// Side-by-Side ペアリング結果の型。各ペアは (左側の行+インデックス, 右側の行+インデックス)。
pub type SideBySidePair<'a> = (Option<(&'a DiffLine, usize)>, Option<(&'a DiffLine, usize)>);

use super::search::apply_search_highlight;
use super::style_utils::{
    base_bg, format_line_num, prefix_style, resolve_bg, style_with_bg, tag_char, truncate_spans,
};

/// 行インデックスが指定ハンクの範囲内にあるかチェック（O(1)）
pub fn is_line_in_hunk(line_index: usize, hunk: &DiffHunk) -> bool {
    hunk.contains_line(line_index)
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
            ("⏎ ", state.palette.dialog_accent)
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

/// Side-by-Side 用に diff 行を左右にペアリングする。
/// 各行に元の lines 配列内でのインデックスを付与する（ハンクハイライト用）。
pub fn split_for_side_by_side(lines: &[DiffLine]) -> Vec<SideBySidePair<'_>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        match lines[i].tag {
            DiffTag::Equal => {
                result.push((Some((&lines[i], i)), Some((&lines[i], i))));
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
                        Some((&lines[delete_start + j], delete_start + j))
                    } else {
                        None
                    };
                    let right = if j < insert_count {
                        Some((&lines[delete_end + j], delete_end + j))
                    } else {
                        None
                    };
                    result.push((left, right));
                }
            }
            DiffTag::Insert => {
                result.push((None, Some((&lines[i], i))));
                i += 1;
            }
        }
    }

    result
}

/// Side-by-Side の片側（左または右）の Span リストを構築する。
///
/// - `None` の場合：空パディング Span を返す
/// - `Some(line)` の場合：行番号 + プレフィックス + ハイライト済みテキスト（幅制限付き）
fn render_sbs_half(
    state: &AppState,
    line_opt: Option<&DiffLine>,
    content_width: usize,
    hunk_bg: Option<Color>,
    cursor_bg: Option<Color>,
) -> Vec<Span<'static>> {
    let p = &state.palette;
    match line_opt {
        Some(line) => {
            let num = format_line_num(match line.tag {
                DiffTag::Delete => line.old_index,
                DiffTag::Insert => line.new_index,
                DiffTag::Equal => line.old_index,
            });
            let prefix = tag_char(line.tag);

            let line_base_bg = base_bg(p, line.tag);
            let bg = hunk_bg.or(line_base_bg).or(cursor_bg);

            let num_style = style_with_bg(Style::default().fg(p.gutter_fg), bg);
            let pstyle = style_with_bg(prefix_style(p, line.tag), bg);
            let gap_style = bg.map(|b| Style::default().bg(b)).unwrap_or_default();

            // ハイライト + 検索ハイライト済み Span を取得し、幅制限 + パディング
            let content_spans = text_spans(state, line, bg);
            let truncated = truncate_spans(content_spans, content_width, bg);

            let mut spans = vec![
                Span::styled(num, num_style),
                Span::styled(prefix.to_string(), pstyle),
                Span::styled(" ", gap_style),
            ];
            spans.extend(truncated);
            spans
        }
        None => {
            let bg = hunk_bg.or(cursor_bg);
            // 行番号(5) + プレフィックス(1) + ギャップ(1) + コンテンツ幅 = content_width + 7
            let empty = format!("{:<width$}", "", width = content_width + 7);
            let empty_style = style_with_bg(Style::default().fg(p.gutter_fg), bg);
            vec![Span::styled(empty, empty_style)]
        }
    }
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

    let (indicator_char, indicator_color) = if is_current_hunk && is_focused {
        if is_pending {
            ("⏎", state.palette.dialog_accent)
        } else {
            ("▶", p.accent)
        }
    } else {
        (" ", Color::Reset)
    };
    let indicator_bg = hunk_bg.or(cursor_bg);
    let indicator_style = style_with_bg(Style::default().fg(indicator_color), indicator_bg);

    let mut spans = vec![Span::styled(indicator_char, indicator_style)];
    spans.extend(render_sbs_half(
        state,
        left,
        content_width,
        hunk_bg,
        cursor_bg,
    ));
    spans.push(Span::styled("│", Style::default().fg(p.gutter_fg)));
    spans.extend(render_sbs_half(
        state,
        right,
        content_width,
        hunk_bg,
        cursor_bg,
    ));

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
        assert_eq!(pairs[0].0.unwrap().0.value, "old");
        assert_eq!(pairs[0].0.unwrap().1, 0); // 元インデックス
        assert_eq!(pairs[0].1.unwrap().0.value, "new");
        assert_eq!(pairs[0].1.unwrap().1, 1); // 元インデックス
    }

    #[test]
    fn test_split_for_side_by_side_delete2_insert3() {
        // Delete×2 + Insert×3 のペアリングで各行の元インデックスが正確
        let lines = vec![
            DiffLine {
                tag: DiffTag::Delete,
                value: "d0".into(),
                old_index: Some(0),
                new_index: None,
            },
            DiffLine {
                tag: DiffTag::Delete,
                value: "d1".into(),
                old_index: Some(1),
                new_index: None,
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "i0".into(),
                old_index: None,
                new_index: Some(0),
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "i1".into(),
                old_index: None,
                new_index: Some(1),
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "i2".into(),
                old_index: None,
                new_index: Some(2),
            },
        ];
        let pairs = split_for_side_by_side(&lines);
        // max(2, 3) = 3 ペア
        assert_eq!(pairs.len(), 3);
        // ペア0: left=d0(idx=0), right=i0(idx=2)
        assert_eq!(pairs[0].0.unwrap().0.value, "d0");
        assert_eq!(pairs[0].0.unwrap().1, 0);
        assert_eq!(pairs[0].1.unwrap().0.value, "i0");
        assert_eq!(pairs[0].1.unwrap().1, 2);
        // ペア1: left=d1(idx=1), right=i1(idx=3)
        assert_eq!(pairs[1].0.unwrap().0.value, "d1");
        assert_eq!(pairs[1].0.unwrap().1, 1);
        assert_eq!(pairs[1].1.unwrap().0.value, "i1");
        assert_eq!(pairs[1].1.unwrap().1, 3);
        // ペア2: left=None, right=i2(idx=4)
        assert!(pairs[2].0.is_none());
        assert_eq!(pairs[2].1.unwrap().0.value, "i2");
        assert_eq!(pairs[2].1.unwrap().1, 4);
    }

    #[test]
    fn test_split_for_side_by_side_insert_only() {
        // Insert単独ブロック（前にDeleteなし）でインデックスが正確
        let lines = vec![
            DiffLine {
                tag: DiffTag::Equal,
                value: "ctx".into(),
                old_index: Some(0),
                new_index: Some(0),
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "new1".into(),
                old_index: None,
                new_index: Some(1),
            },
            DiffLine {
                tag: DiffTag::Insert,
                value: "new2".into(),
                old_index: None,
                new_index: Some(2),
            },
        ];
        let pairs = split_for_side_by_side(&lines);
        assert_eq!(pairs.len(), 3);
        // ペア0: Equal(idx=0)
        assert_eq!(pairs[0].0.unwrap().1, 0);
        assert_eq!(pairs[0].1.unwrap().1, 0);
        // ペア1: left=None, right=new1(idx=1)
        assert!(pairs[1].0.is_none());
        assert_eq!(pairs[1].1.unwrap().0.value, "new1");
        assert_eq!(pairs[1].1.unwrap().1, 1);
        // ペア2: left=None, right=new2(idx=2)
        assert!(pairs[2].0.is_none());
        assert_eq!(pairs[2].1.unwrap().0.value, "new2");
        assert_eq!(pairs[2].1.unwrap().1, 2);
    }

    // --- render_sbs_half tests ---

    #[test]
    fn test_sbs_half_none_returns_empty_padding() {
        let state = make_test_state();
        let content_width = 20;
        let spans = render_sbs_half(&state, None, content_width, None, None);
        // None → 単一の空パディング Span（content_width + 7 幅）
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.len(), content_width + 7);
    }

    #[test]
    fn test_sbs_half_highlight_enabled_with_cache() {
        use crate::highlight::engine::StyledSegment;
        use ratatui::style::Modifier;

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

        let content_width = 30;
        let spans = render_sbs_half(&state, Some(&line), content_width, None, None);
        // 行番号(1) + プレフィックス(1) + ギャップ(1) + ハイライト Span(2) + パディング(1) = 6 以上
        assert!(
            spans.len() >= 5,
            "ハイライト有効+キャッシュあり → 複数 Span 生成 (got {})",
            spans.len()
        );
        // 先頭3つは行番号・プレフィックス・ギャップ
        // 4番目以降がハイライト済みテキスト
        let fn_span = &spans[3];
        assert_eq!(fn_span.style.fg, Some(Color::Rgb(180, 142, 173)));
    }

    #[test]
    fn test_sbs_half_highlight_disabled_fallback() {
        let mut state = make_test_state();
        state.syntax_highlight_enabled = false;

        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "hello".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };

        let content_width = 20;
        let spans = render_sbs_half(&state, Some(&line), content_width, None, None);
        // 行番号(1) + プレフィックス(1) + ギャップ(1) + テキスト Span(1) + パディング(1) = 5
        // テキスト部分はプレーンテキストの単一 Span + パディング
        assert!(
            spans.len() >= 4,
            "ハイライト無効 → フォールバック Span (got {})",
            spans.len()
        );
        // テキスト部分にプレーンテキストが含まれる
        let text_content: String = spans[3..].iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text_content.starts_with("hello"),
            "フォールバック時はプレーンテキストが含まれるべき"
        );
    }

    #[test]
    fn test_sbs_half_no_cache_fallback() {
        let mut state = make_test_state();
        // ハイライト有効だがキャッシュなし → フォールバック
        state.syntax_highlight_enabled = true;
        state.selected_path = Some("test.rs".to_string());
        // highlight_cache_left は空のまま

        let line = DiffLine {
            tag: DiffTag::Delete,
            value: "fn main".to_string(),
            old_index: Some(0),
            new_index: None,
        };

        let content_width = 20;
        let spans = render_sbs_half(&state, Some(&line), content_width, None, None);
        // キャッシュミス → plain_text_span フォールバック
        // 行番号(1) + プレフィックス(1) + ギャップ(1) + テキスト(1) + パディング(1) = 5
        assert!(
            spans.len() >= 4,
            "キャッシュなし → フォールバック Span (got {})",
            spans.len()
        );
        let text_content: String = spans[3..].iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text_content.starts_with("fn main"),
            "フォールバック時はプレーンテキストが含まれるべき"
        );
    }

    #[test]
    fn test_sbs_half_search_highlight_applied() {
        let mut state = make_test_state();
        state.diff_search_state.query = "hello".to_string();

        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "say hello world".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };

        let content_width = 30;
        let spans = render_sbs_half(&state, Some(&line), content_width, None, None);
        // 検索クエリあり → text_spans 内で検索ハイライトが適用される
        // 元の1 Span が分割されるので、テキスト部分の Span 数 > 1
        let text_spans: Vec<&Span> = spans[3..].iter().collect();
        assert!(
            text_spans.len() > 1,
            "検索ハイライトにより Span が分割されるべき (got {})",
            text_spans.len()
        );
        // 検索マッチ部分に accent 色の背景が適用されている
        let has_accent_bg = text_spans
            .iter()
            .any(|s| s.style.bg == Some(state.palette.accent));
        assert!(has_accent_bg, "検索マッチに accent 背景色が適用されるべき");
    }

    #[test]
    fn test_sbs_half_equal_line_same_highlight_both_sides() {
        use crate::highlight::engine::StyledSegment;
        use ratatui::style::Modifier;

        let mut state = make_test_state();
        state.selected_path = Some("test.rs".to_string());
        // Equal 行は left キャッシュを使う（get_highlight_segments の仕様）
        let highlight_data = vec![vec![
            StyledSegment {
                text: "let".to_string(),
                fg: Some(Color::Rgb(180, 142, 173)),
                modifier: Modifier::empty(),
            },
            StyledSegment {
                text: " x = 1".to_string(),
                fg: Some(Color::Rgb(200, 200, 200)),
                modifier: Modifier::empty(),
            },
        ]];
        state
            .highlight_cache_left
            .insert("test.rs".to_string(), highlight_data);

        let line = DiffLine {
            tag: DiffTag::Equal,
            value: "let x = 1".to_string(),
            old_index: Some(0),
            new_index: Some(0),
        };

        let content_width = 30;
        let left_spans = render_sbs_half(&state, Some(&line), content_width, None, None);
        let right_spans = render_sbs_half(&state, Some(&line), content_width, None, None);

        // Equal 行は左右とも同じキャッシュ（left）を参照するため、同一の Span が生成される
        assert_eq!(left_spans.len(), right_spans.len());
        for (l, r) in left_spans.iter().zip(right_spans.iter()) {
            assert_eq!(l.content, r.content);
            assert_eq!(l.style, r.style);
        }
    }
}
