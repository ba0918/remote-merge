//! Diff ビュー用のスタイルユーティリティ。

use ratatui::style::{Color, Modifier, Style};
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

/// 表示幅ベースでトランケーション or パディングする
pub fn truncate_or_pad(value: &str, width: usize) -> String {
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
}
