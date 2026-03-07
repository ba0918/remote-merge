//! ファイル名インクリメンタルサーチのロジック。
//!
//! 純粋関数で構成し、UI やイベント処理から独立してテスト可能にする。

use super::types::{FlatNode, MergedNode};

/// ファイル検索の状態
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    /// 検索モードが有効か
    pub active: bool,
    /// 検索クエリ文字列
    pub query: String,
    /// 一致したインデックス群の中での現在位置
    pub match_cursor: usize,
}

impl SearchState {
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

/// ノード名が検索クエリにマッチするか（case-insensitive 部分一致）。
///
/// `query_lower` は事前に `to_lowercase()` 済みの値を渡す。
pub fn name_matches(name: &str, query_lower: &str) -> bool {
    name.to_lowercase().contains(query_lower)
}

/// flat_nodes から query にマッチするインデックスのリストを返す（case-insensitive）。
///
/// ノード名（`name` フィールド）に対して部分一致で検索する。
pub fn find_matches(flat_nodes: &[FlatNode], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    flat_nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| name_matches(&node.name, &query_lower))
        .map(|(i, _)| i)
        .collect()
}

/// match_cursor を次に進める（循環）。
///
/// total が 0 の場合は 0 を返す。
pub fn next_match(current: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    (current + 1) % total
}

/// match_cursor を前に戻す（循環）。
///
/// total が 0 の場合は 0 を返す。
pub fn prev_match(current: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    if current == 0 {
        total - 1
    } else {
        current - 1
    }
}

/// MergedNode ツリーのディレクトリ配下に検索クエリにマッチするノードが存在するか（再帰チェック）。
///
/// ツリーフィルタリング時にマッチする子孫がいないディレクトリをスキップするために使用する。
pub fn dir_has_search_matches(node: &MergedNode, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query_lower = query.to_lowercase();
    dir_has_search_matches_lower(node, &query_lower)
}

/// `query_lower` 事前計算済み版の内部関数。再帰で `to_lowercase()` を繰り返さない。
fn dir_has_search_matches_lower(node: &MergedNode, query_lower: &str) -> bool {
    for child in &node.children {
        if child.is_dir {
            if dir_has_search_matches_lower(child, query_lower) {
                return true;
            }
        } else if name_matches(&child.name, query_lower) {
            return true;
        }
    }
    false
}

/// 検索ステータスメッセージを生成する（一致なし / クエリのみ）。
pub fn format_search_status(query: &str, total: usize) -> String {
    if total == 0 && !query.is_empty() {
        format!("/{} [no match]", query)
    } else {
        format!("/{}", query)
    }
}

/// 検索ステータスメッセージを生成する（位置付き）。
pub fn format_search_status_with_pos(query: &str, pos: usize, total: usize) -> String {
    format!("/{} [{}/{}]", query, pos, total)
}

/// 現在のカーソル位置から最も近い一致を探す。
///
/// `matches` は flat_nodes 上のインデックスのソート済みリスト。
/// `cursor` は現在の tree_cursor 位置。
/// カーソル位置以降で最初の一致を返し、なければ先頭に戻る。
pub fn nearest_match_from(matches: &[usize], cursor: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    // cursor 以降の最初の一致
    for (i, &idx) in matches.iter().enumerate() {
        if idx >= cursor {
            return Some(i);
        }
    }
    // 見つからなければ先頭（循環）
    Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, FlatNode};

    fn make_node(name: &str) -> FlatNode {
        FlatNode {
            path: format!("src/{}", name),
            name: name.to_string(),
            depth: 1,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
            ref_only: false,
        }
    }

    // ── find_matches ──

    #[test]
    fn test_find_matches_partial() {
        let nodes = vec![
            make_node("main.rs"),
            make_node("lib.rs"),
            make_node("main_test.rs"),
        ];
        let matches = find_matches(&nodes, "main");
        assert_eq!(matches, vec![0, 2]);
    }

    #[test]
    fn test_find_matches_case_insensitive() {
        let nodes = vec![make_node("README.md"), make_node("readme.txt")];
        let matches = find_matches(&nodes, "readme");
        assert_eq!(matches, vec![0, 1]);
    }

    #[test]
    fn test_find_matches_empty_query() {
        let nodes = vec![make_node("main.rs")];
        let matches = find_matches(&nodes, "");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_matches_no_match() {
        let nodes = vec![make_node("main.rs"), make_node("lib.rs")];
        let matches = find_matches(&nodes, "xyz");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_matches_directory() {
        let mut dir = make_node("src");
        dir.is_dir = true;
        let nodes = vec![dir, make_node("main.rs")];
        let matches = find_matches(&nodes, "src");
        assert_eq!(matches, vec![0]);
    }

    // ── next_match / prev_match ──

    #[test]
    fn test_next_match_wraps() {
        assert_eq!(next_match(2, 3), 0);
    }

    #[test]
    fn test_next_match_normal() {
        assert_eq!(next_match(0, 3), 1);
    }

    #[test]
    fn test_next_match_zero_total() {
        assert_eq!(next_match(0, 0), 0);
    }

    #[test]
    fn test_prev_match_wraps() {
        assert_eq!(prev_match(0, 3), 2);
    }

    #[test]
    fn test_prev_match_normal() {
        assert_eq!(prev_match(2, 3), 1);
    }

    #[test]
    fn test_prev_match_zero_total() {
        assert_eq!(prev_match(0, 0), 0);
    }

    // ── nearest_match_from ──

    #[test]
    fn test_nearest_match_from_cursor() {
        let matches = vec![2, 5, 8];
        assert_eq!(nearest_match_from(&matches, 3), Some(1)); // 5 が最初の >= 3
    }

    #[test]
    fn test_nearest_match_from_wraps() {
        let matches = vec![2, 5, 8];
        assert_eq!(nearest_match_from(&matches, 9), Some(0)); // 循環して先頭
    }

    #[test]
    fn test_nearest_match_from_exact() {
        let matches = vec![2, 5, 8];
        assert_eq!(nearest_match_from(&matches, 5), Some(1));
    }

    #[test]
    fn test_nearest_match_from_empty() {
        let matches: Vec<usize> = vec![];
        assert_eq!(nearest_match_from(&matches, 0), None);
    }

    // ── SearchState ──

    #[test]
    fn test_search_state_activate() {
        let mut state = SearchState {
            query: "old".to_string(),
            match_cursor: 5,
            ..Default::default()
        };
        state.activate();
        assert!(state.active);
        assert!(state.query.is_empty());
        assert_eq!(state.match_cursor, 0);
    }

    #[test]
    fn test_search_state_deactivate_keeps_query() {
        let mut state = SearchState {
            active: true,
            query: "test".to_string(),
            match_cursor: 2,
        };
        state.deactivate();
        assert!(!state.active);
        assert_eq!(state.query, "test");
    }

    #[test]
    fn test_search_state_clear() {
        let mut state = SearchState {
            active: true,
            query: "test".to_string(),
            match_cursor: 2,
        };
        state.clear();
        assert!(!state.active);
        assert!(state.query.is_empty());
        assert_eq!(state.match_cursor, 0);
    }

    // ── dir_has_search_matches ──

    fn make_merged_file(name: &str) -> MergedNode {
        MergedNode {
            name: name.to_string(),
            is_dir: false,
            is_symlink: false,
            children: vec![],
            ref_only: false,
        }
    }

    fn make_merged_dir(name: &str, children: Vec<MergedNode>) -> MergedNode {
        MergedNode {
            name: name.to_string(),
            is_dir: true,
            is_symlink: false,
            children,
            ref_only: false,
        }
    }

    #[test]
    fn test_dir_has_search_matches_direct_child() {
        let dir = make_merged_dir(
            "src",
            vec![make_merged_file("main.rs"), make_merged_file("lib.rs")],
        );
        assert!(dir_has_search_matches(&dir, "main"));
        assert!(!dir_has_search_matches(&dir, "xyz"));
    }

    #[test]
    fn test_dir_has_search_matches_nested() {
        let dir = make_merged_dir(
            "src",
            vec![make_merged_dir("app", vec![make_merged_file("search.rs")])],
        );
        assert!(dir_has_search_matches(&dir, "search"));
        assert!(!dir_has_search_matches(&dir, "xyz"));
    }

    #[test]
    fn test_dir_has_search_matches_empty_query() {
        let dir = make_merged_dir("src", vec![]);
        assert!(dir_has_search_matches(&dir, ""));
    }

    #[test]
    fn test_dir_has_search_matches_case_insensitive() {
        let dir = make_merged_dir("src", vec![make_merged_file("README.md")]);
        assert!(dir_has_search_matches(&dir, "readme"));
    }
}
