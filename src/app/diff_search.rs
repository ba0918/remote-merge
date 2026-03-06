//! Diff View 内テキスト検索のロジック。
//!
//! ファイル名検索（search.rs）とは独立した、diff コンテンツ内の文字列検索。
//! 共通の SearchState / カーソル操作は search.rs を再利用する。

use crate::diff::engine::DiffLine;

/// diff 行リストから query にマッチする行インデックスを返す（case-insensitive）。
pub fn find_diff_matches(lines: &[DiffLine], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.value.to_lowercase().contains(&query_lower))
        .map(|(i, _)| i)
        .collect()
}

/// Equal コンテンツ行から query にマッチする行インデックスを返す（case-insensitive）。
pub fn find_content_matches(content: &str, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| line.to_lowercase().contains(&query_lower))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::engine::DiffTag;

    fn make_diff_line(value: &str, tag: DiffTag) -> DiffLine {
        DiffLine {
            tag,
            value: value.to_string(),
            old_index: Some(0),
            new_index: Some(0),
        }
    }

    #[test]
    fn test_find_diff_matches_basic() {
        let lines = vec![
            make_diff_line("hello world", DiffTag::Equal),
            make_diff_line("foo bar", DiffTag::Insert),
            make_diff_line("hello again", DiffTag::Delete),
        ];
        let matches = find_diff_matches(&lines, "hello");
        assert_eq!(matches, vec![0, 2]);
    }

    #[test]
    fn test_find_diff_matches_case_insensitive() {
        let lines = vec![
            make_diff_line("Hello World", DiffTag::Equal),
            make_diff_line("HELLO", DiffTag::Insert),
        ];
        let matches = find_diff_matches(&lines, "hello");
        assert_eq!(matches, vec![0, 1]);
    }

    #[test]
    fn test_find_diff_matches_empty_query() {
        let lines = vec![make_diff_line("hello", DiffTag::Equal)];
        assert!(find_diff_matches(&lines, "").is_empty());
    }

    #[test]
    fn test_find_diff_matches_no_match() {
        let lines = vec![make_diff_line("hello", DiffTag::Equal)];
        assert!(find_diff_matches(&lines, "xyz").is_empty());
    }

    #[test]
    fn test_find_content_matches() {
        let content = "hello world\nfoo bar\nhello again";
        let matches = find_content_matches(content, "hello");
        assert_eq!(matches, vec![0, 2]);
    }
}
