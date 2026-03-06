//! Diff View 内テキスト検索のロジック。
//!
//! ファイル名検索（search.rs）とは独立した、diff コンテンツ内の文字列検索。

use crate::diff::engine::DiffLine;

/// Diff View テキスト検索の状態
#[derive(Debug, Clone, Default)]
pub struct DiffSearchState {
    /// 検索モードが有効か
    pub active: bool,
    /// 検索クエリ文字列
    pub query: String,
    /// マッチした行インデックス群の中での現在位置
    pub match_cursor: usize,
}

impl DiffSearchState {
    /// 検索を開始する
    pub fn activate(&mut self) {
        self.active = true;
        self.query.clear();
        self.match_cursor = 0;
    }

    /// 検索を終了する（結果は保持）
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// 検索を終了し結果もクリアする
    pub fn clear(&mut self) {
        self.active = false;
        self.query.clear();
        self.match_cursor = 0;
    }

    /// クエリが空でないか
    pub fn has_query(&self) -> bool {
        !self.query.is_empty()
    }
}

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

/// match_cursor を次に進める（循環）。
pub fn next_diff_match(current: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    (current + 1) % total
}

/// match_cursor を前に戻す（循環）。
pub fn prev_diff_match(current: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    if current == 0 {
        total - 1
    } else {
        current - 1
    }
}

/// カーソル位置から最も近いマッチを探す。
pub fn nearest_diff_match_from(matches: &[usize], cursor: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    for (i, &idx) in matches.iter().enumerate() {
        if idx >= cursor {
            return Some(i);
        }
    }
    Some(0)
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

    #[test]
    fn test_next_prev_diff_match() {
        assert_eq!(next_diff_match(0, 3), 1);
        assert_eq!(next_diff_match(2, 3), 0);
        assert_eq!(prev_diff_match(0, 3), 2);
        assert_eq!(prev_diff_match(1, 3), 0);
    }

    #[test]
    fn test_nearest_diff_match_from() {
        let matches = vec![2, 5, 8];
        assert_eq!(nearest_diff_match_from(&matches, 3), Some(1));
        assert_eq!(nearest_diff_match_from(&matches, 9), Some(0));
        assert_eq!(nearest_diff_match_from(&matches, 5), Some(1));
    }

    #[test]
    fn test_diff_search_state_lifecycle() {
        let mut state = DiffSearchState::default();
        assert!(!state.active);

        state.activate();
        assert!(state.active);
        assert!(state.query.is_empty());

        state.query = "test".to_string();
        state.deactivate();
        assert!(!state.active);
        assert_eq!(state.query, "test");

        state.clear();
        assert!(state.query.is_empty());
    }
}
