//! Diff 検索ハイライト処理。

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Span リストに対して検索クエリのマッチ部分をハイライトする。
///
/// 各 Span のテキストを case-insensitive で分割し、マッチ部分に
/// 黒文字+アクセント背景のスタイルを適用する。
pub fn apply_search_highlight<'a>(
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

    #[test]
    fn test_no_match() {
        let spans = vec![Span::raw("hello world")];
        let result = apply_search_highlight(spans, "xyz", Color::Yellow);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content.as_ref(), "hello world");
    }

    #[test]
    fn test_empty_query() {
        let spans = vec![Span::raw("hello")];
        let result = apply_search_highlight(spans, "", Color::Yellow);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_single_match() {
        let spans = vec![Span::raw("hello world")];
        let result = apply_search_highlight(spans, "world", Color::Yellow);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content.as_ref(), "hello ");
        assert_eq!(result[1].content.as_ref(), "world");
        assert_eq!(result[1].style.bg, Some(Color::Yellow));
    }

    #[test]
    fn test_case_insensitive() {
        let spans = vec![Span::raw("Hello World")];
        let result = apply_search_highlight(spans, "hello", Color::Yellow);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content.as_ref(), "Hello");
        assert_eq!(result[0].style.bg, Some(Color::Yellow));
    }

    #[test]
    fn test_multiple_spans() {
        let spans = vec![Span::raw("abc"), Span::raw("def")];
        let result = apply_search_highlight(spans, "c", Color::Yellow);
        assert_eq!(result.len(), 3); // "ab", "c", "def"
        assert_eq!(result[1].content.as_ref(), "c");
        assert_eq!(result[1].style.bg, Some(Color::Yellow));
    }
}
