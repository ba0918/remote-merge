//! Diff ビュー用のスタイルユーティリティ。

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use unicode_width::UnicodeWidthChar;
#[cfg(test)]
use unicode_width::UnicodeWidthStr;

use crate::diff::engine::DiffTag;
use crate::theme::TuiPalette;

/// Style にオプショナルな背景色を適用する。
pub fn style_with_bg(style: Style, bg: Option<Color>) -> Style {
    match bg {
        Some(bg) => style.bg(bg),
        None => style,
    }
}

/// 表示幅ベースでトランケーション or パディングする（テスト用に残存）
#[cfg(test)]
fn truncate_or_pad(value: &str, width: usize) -> String {
    let display_width = UnicodeWidthStr::width(value);
    if display_width > width {
        // 表示幅で切る（文字境界を壊さない）
        let target = width.saturating_sub(1); // 「…」分
        let mut current_width = 0;
        let mut end = 0;
        for (i, ch) in value.char_indices() {
            let ch_width = char_width(ch);
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

/// ハイライト済み `Vec<Span>` を表示幅に収まるようトランケートし、
/// 残り幅をスペースでパディングする。
///
/// - 幅に収まる Span はそのまま移動（clone なし）
/// - 幅超過時は文字単位で切り詰め、残り幅 ≥ 1 なら末尾に「…」付与
/// - 全角文字が幅境界をまたぐ場合はスペースで埋める
/// - パディング Span の bg は `pad_bg` で指定
pub fn truncate_spans(
    spans: Vec<Span<'static>>,
    width: usize,
    pad_bg: Option<Color>,
) -> Vec<Span<'static>> {
    let mut result = Vec::with_capacity(spans.len() + 1);
    let mut used = 0;

    if width == 0 {
        let pad_style = style_with_bg(Style::default(), pad_bg);
        result.push(Span::styled("", pad_style));
        return result;
    }

    for span in spans {
        if used >= width {
            break;
        }

        let span_width = span_display_width(&span);

        if used + span_width <= width {
            // Span 全体が収まる
            used += span_width;
            result.push(span);
        } else {
            // この Span の途中でトランケーションが必要
            let remaining = width - used;
            let (truncated, consumed_width) = truncate_span_content(&span, remaining);
            result.push(Span::styled(truncated, span.style));
            used += consumed_width;
            break;
        }
    }

    // 残り幅をパディング
    let remaining = width.saturating_sub(used);
    if remaining > 0 {
        let pad_style = style_with_bg(Style::default(), pad_bg);
        result.push(Span::styled(" ".repeat(remaining), pad_style));
    }

    result
}

/// Span 内テキストの表示幅を計算する。タブは幅1として扱う。
fn span_display_width(span: &Span<'_>) -> usize {
    span.content.chars().map(char_width).sum()
}

/// 文字の表示幅を返す。タブ等 `UnicodeWidthChar` が None を返す文字は幅1扱い。
fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// Span の内容を `remaining` 幅に収まるようトランケートする。
/// 残り幅 ≥ 1 なら末尾に「…」を付与。
/// 全角文字が幅境界をまたぐ場合はスペースで埋める。
/// 返り値: (トランケート済み文字列, 消費した表示幅)
fn truncate_span_content(span: &Span<'_>, remaining: usize) -> (String, usize) {
    // 「…」分を確保
    let target = remaining.saturating_sub(1);
    let mut current_width = 0;
    let mut buf = String::with_capacity(remaining);

    for ch in span.content.chars() {
        let cw = char_width(ch);
        if current_width + cw > target {
            break;
        }
        current_width += cw;
        buf.push(ch);
    }

    // 残り幅 ≥ 1 なら「…」付与
    if remaining >= 1 {
        buf.push('…');
        current_width += 1;
    }

    // 全角文字の幅境界またぎ等で隙間が残る場合、スペースで埋める
    let gap = remaining.saturating_sub(current_width);
    if gap > 0 {
        buf.extend(std::iter::repeat_n(' ', gap));
        current_width += gap;
    }

    (buf, current_width)
}

/// 行番号をフォーマット（5桁右寄せ）
pub fn format_line_num(num: Option<usize>) -> String {
    match num {
        Some(n) => format!("{:>5}", n + 1),
        None => "     ".to_string(),
    }
}

/// diff タグのプレフィックス文字
pub fn tag_char(tag: DiffTag) -> &'static str {
    match tag {
        DiffTag::Equal => " ",
        DiffTag::Insert => "+",
        DiffTag::Delete => "-",
    }
}

/// diff タグに応じたベース背景色をパレットから取得
pub fn base_bg(palette: &TuiPalette, tag: DiffTag) -> Option<Color> {
    match tag {
        DiffTag::Insert => Some(palette.diff_insert_bg),
        DiffTag::Delete => Some(palette.diff_delete_bg),
        DiffTag::Equal => None,
    }
}

/// diff タグに応じたプレフィックスのスタイル（パレット版）
pub fn prefix_style(palette: &TuiPalette, tag: DiffTag) -> Style {
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
pub fn resolve_bg(
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
        let base = base_bg(palette, tag);
        if base.is_some() {
            base
        } else if is_cursor_line && is_focused {
            Some(palette.cursor_line_bg)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_style_with_bg_some() {
        let style = Style::default().fg(Color::Red);
        let result = style_with_bg(style, Some(Color::Blue));
        assert_eq!(result.bg, Some(Color::Blue));
        assert_eq!(result.fg, Some(Color::Red));
    }

    #[test]
    fn test_style_with_bg_none() {
        let style = Style::default().fg(Color::Red);
        let result = style_with_bg(style, None);
        assert_eq!(result.bg, None);
        assert_eq!(result.fg, Some(Color::Red));
    }

    #[test]
    fn test_format_line_num() {
        assert_eq!(format_line_num(Some(0)), "    1");
        assert_eq!(format_line_num(Some(99)), "  100");
        assert_eq!(format_line_num(None), "     ");
    }

    #[test]
    fn test_tag_char() {
        assert_eq!(tag_char(DiffTag::Equal), " ");
        assert_eq!(tag_char(DiffTag::Insert), "+");
        assert_eq!(tag_char(DiffTag::Delete), "-");
    }

    fn test_palette() -> TuiPalette {
        let theme = crate::theme::load_theme(crate::theme::DEFAULT_THEME);
        TuiPalette::from_theme(&theme)
    }

    #[test]
    fn test_resolve_bg_current_hunk_focused() {
        let palette = test_palette();
        let bg = resolve_bg(&palette, DiffTag::Equal, true, true, false, false);
        assert_eq!(bg, Some(palette.hunk_select_bg));
    }

    #[test]
    fn test_resolve_bg_current_hunk_pending() {
        let palette = test_palette();
        let bg = resolve_bg(&palette, DiffTag::Equal, true, true, true, false);
        assert_eq!(bg, Some(palette.hunk_pending_bg));
    }

    #[test]
    fn test_resolve_bg_insert_no_hunk() {
        let palette = test_palette();
        let bg = resolve_bg(&palette, DiffTag::Insert, false, true, false, false);
        assert_eq!(bg, Some(palette.diff_insert_bg));
    }

    #[test]
    fn test_resolve_bg_cursor_line() {
        let palette = test_palette();
        let bg = resolve_bg(&palette, DiffTag::Equal, false, true, false, true);
        assert_eq!(bg, Some(palette.cursor_line_bg));
    }

    // --- truncate_spans tests ---

    /// 結果の Span 全体の表示幅を計算するヘルパー
    fn total_display_width(spans: &[Span<'_>]) -> usize {
        spans.iter().map(|s| span_display_width(s)).sum()
    }

    /// 結果の Span のテキストを連結するヘルパー
    fn concat_text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn test_truncate_spans_ascii_fits() {
        // ASCII Span が幅内に収まる → パディング付きで全体幅一致
        let spans = vec![Span::raw("hello")];
        let result = truncate_spans(spans, 10, None);
        assert_eq!(total_display_width(&result), 10);
        assert_eq!(concat_text(&result), "hello     ");
    }

    #[test]
    fn test_truncate_spans_ascii_truncation() {
        // ASCII Span が幅超過 → トランケーション + 「…」+ パディング
        let spans = vec![Span::raw("hello world")];
        let result = truncate_spans(spans, 8, None);
        assert_eq!(total_display_width(&result), 8);
        let text = concat_text(&result);
        assert!(text.contains('…'));
        assert_eq!(UnicodeWidthStr::width(text.as_str()), 8);
    }

    #[test]
    fn test_truncate_spans_multi_span_truncation() {
        // 複数 Span をまたぐトランケーション（2番目の Span の途中で切れる）
        let spans = vec![
            Span::styled("abc", Style::default().fg(Color::Red)),
            Span::styled("defgh", Style::default().fg(Color::Blue)),
        ];
        let result = truncate_spans(spans, 6, None);
        assert_eq!(total_display_width(&result), 6);
        // 1番目 "abc" (幅3) はそのまま、2番目 "defgh" は幅3に切り詰め → "de…"
        assert_eq!(result[0].content.as_ref(), "abc");
        assert!(result[1].content.contains('…'));
    }

    #[test]
    fn test_truncate_spans_fullwidth_truncation() {
        // 全角文字のトランケーション（文字境界を壊さない）
        let spans = vec![Span::raw("あいうえお")]; // 幅10
        let result = truncate_spans(spans, 7, None);
        assert_eq!(total_display_width(&result), 7);
        let text = concat_text(&result);
        assert!(text.contains('…'));
    }

    #[test]
    fn test_truncate_spans_fullwidth_boundary_straddle() {
        // 全角文字が幅境界をまたぐ（残り幅1で全角幅2）→ スペース埋め
        // "あい" = 幅4, 幅5に切り詰め → "あい" (4) + padding (1) ではなく、
        // "あいう" (幅6) を幅5に切り詰め → target=4, "あい"(4) + "…"(1) = 5
        let spans = vec![Span::raw("あいう")]; // 幅6
        let result = truncate_spans(spans, 5, None);
        assert_eq!(total_display_width(&result), 5);
        let text = concat_text(&result);
        assert!(text.contains('…'));

        // "aあ" = 幅3 を幅2に切り詰め → target=1, "a"(1) は入るが "あ"(2)は入らない
        // → "a…" = 幅2
        let spans = vec![Span::raw("aあ")]; // 幅3
        let result = truncate_spans(spans, 2, None);
        assert_eq!(total_display_width(&result), 2);
        let text = concat_text(&result);
        assert_eq!(text, "a…");

        // 全角のみで幅境界をまたぐ: "あい" (幅4) を幅3に → target=2, "あ"(2) + "…"(1) = 3
        // 全角が入らないケース: "あい" (幅4) を幅2に → target=1, "あ"は幅2で入らない → "…" + スペース1
        let spans = vec![Span::raw("あい")]; // 幅4
        let result = truncate_spans(spans, 2, None);
        assert_eq!(total_display_width(&result), 2);
        let text = concat_text(&result);
        assert_eq!(text, "… "); // 「…」(1) + スペース(1)
    }

    #[test]
    fn test_truncate_spans_empty_span_list() {
        // 空 Span リストの処理 → パディングのみ
        let spans: Vec<Span<'static>> = vec![];
        let result = truncate_spans(spans, 5, None);
        assert_eq!(total_display_width(&result), 5);
        assert_eq!(concat_text(&result), "     ");
    }

    #[test]
    fn test_truncate_spans_width_zero() {
        // 幅0の場合 → 空文字列パディング
        let spans = vec![Span::raw("hello")];
        let result = truncate_spans(spans, 0, None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content.as_ref(), "");
    }

    #[test]
    fn test_truncate_spans_width_one() {
        // 幅1の場合 → 「…」のみ（内容なし）
        let spans = vec![Span::raw("hello")];
        let result = truncate_spans(spans, 1, None);
        assert_eq!(total_display_width(&result), 1);
        let text = concat_text(&result);
        assert_eq!(text, "…");
    }

    #[test]
    fn test_truncate_spans_width_two() {
        // 幅2の場合 → 1文字 + 「…」
        let spans = vec![Span::raw("hello")];
        let result = truncate_spans(spans, 2, None);
        assert_eq!(total_display_width(&result), 2);
        let text = concat_text(&result);
        assert_eq!(text, "h…");
    }

    #[test]
    fn test_truncate_spans_tab_character() {
        // タブ文字を含む Span の幅計算（幅1扱い）
        let spans = vec![Span::raw("a\tb")]; // 幅3 (a=1, tab=1, b=1)
        let result = truncate_spans(spans, 5, None);
        assert_eq!(total_display_width(&result), 5);
        assert_eq!(concat_text(&result), "a\tb  ");
    }

    #[test]
    fn test_truncate_spans_pad_bg_some() {
        // pad_bg が Some の場合 → パディング Span に bg が設定される
        let spans = vec![Span::raw("hi")];
        let result = truncate_spans(spans, 5, Some(Color::DarkGray));
        assert_eq!(total_display_width(&result), 5);
        // 最後の Span がパディング
        let pad_span = result.last().unwrap();
        assert_eq!(pad_span.style.bg, Some(Color::DarkGray));
        assert_eq!(pad_span.content.as_ref(), "   ");
    }

    #[test]
    fn test_truncate_spans_pad_bg_none() {
        // pad_bg が None の場合 → パディング Span に bg なし
        let spans = vec![Span::raw("hi")];
        let result = truncate_spans(spans, 5, None);
        let pad_span = result.last().unwrap();
        assert_eq!(pad_span.style.bg, None);
    }

    #[test]
    fn test_truncate_spans_multi_style_at_boundary() {
        // 検索マッチがトランケーション境界をまたぐケース
        let spans = vec![
            Span::styled("abc", Style::default().fg(Color::White)),
            Span::styled("DEF", Style::default().fg(Color::Yellow).bg(Color::Red)),
            Span::styled("ghi", Style::default().fg(Color::White)),
        ];
        // 幅5 → "abc" (3) + "DEF" の途中で切れる（残り2 → target=1 → "D…"）
        let result = truncate_spans(spans, 5, None);
        assert_eq!(total_display_width(&result), 5);
        // 1番目は元のスタイルを保持
        assert_eq!(result[0].style.fg, Some(Color::White));
        assert_eq!(result[0].content.as_ref(), "abc");
        // 2番目はトランケートされたが元のスタイルを保持
        assert_eq!(result[1].style.fg, Some(Color::Yellow));
        assert_eq!(result[1].style.bg, Some(Color::Red));
        assert!(result[1].content.contains('…'));
    }

    #[test]
    fn test_truncate_spans_matches_truncate_or_pad() {
        // フォールバック時（単一プレーンテキスト Span）の出力が既存の truncate_or_pad と同等
        let text = "hello world this is a test";
        let width = 15;

        let expected = truncate_or_pad(text, width);
        let spans = vec![Span::raw(text.to_string())];
        let result = truncate_spans(spans, width, None);
        let actual = concat_text(&result);

        assert_eq!(
            UnicodeWidthStr::width(actual.as_str()),
            UnicodeWidthStr::width(expected.as_str()),
        );
        assert_eq!(actual, expected);

        // パディングのみのケースも同等
        let text2 = "short";
        let expected2 = truncate_or_pad(text2, width);
        let spans2 = vec![Span::raw(text2.to_string())];
        let result2 = truncate_spans(spans2, width, None);
        let actual2 = concat_text(&result2);
        assert_eq!(actual2, expected2);

        // タブ文字を含むケース（char_width 統一の検証）
        let tab_text = "a\tb\tcdef";
        for w in [3, 5, 8, 15] {
            let expected_tab = truncate_or_pad(tab_text, w);
            let spans_tab = vec![Span::raw(tab_text.to_string())];
            let result_tab = truncate_spans(spans_tab, w, None);
            let actual_tab = concat_text(&result_tab);
            assert_eq!(
                actual_tab, expected_tab,
                "tab mismatch at width={w}: spans={actual_tab:?} vs pad={expected_tab:?}"
            );
        }
    }
}
