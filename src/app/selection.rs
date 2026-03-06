//! ファイル選択・読み込み・キャッシュ管理。

use std::path::Path;

use crate::diff::engine::{self, DiffResult};

use super::AppState;

impl AppState {
    /// 現在カーソル位置のファイルを選択して diff を計算する
    pub fn select_file(&mut self) {
        let node = match self.flat_nodes.get(self.tree_cursor) {
            Some(n) if !n.is_dir => n,
            _ => return,
        };
        let path = node.path.clone();
        let is_symlink = node.is_symlink;

        self.selected_path = Some(path.clone());

        // シンボリックリンクの場合はリンク先パスを比較
        if is_symlink {
            self.select_symlink(&path);
            return;
        }

        // バイナリキャッシュにあればそちらを優先（コンテンツを保持しない）
        let local_bin = self.local_binary_cache.get(&path).cloned();
        let remote_bin = self.remote_binary_cache.get(&path).cloned();
        if local_bin.is_some() || remote_bin.is_some() {
            self.current_diff = Some(DiffResult::Binary {
                left: local_bin,
                right: remote_bin,
            });
            self.diff_scroll = 0;
            self.diff_cursor = 0;
            self.hunk_cursor = 0;
            self.pending_hunk_merge = None;
            return;
        }

        // テキストキャッシュからコンテンツを取得して diff
        let local_content = self.local_cache.get(&path).map(|s| s.as_str());
        let remote_content = self.remote_cache.get(&path).map(|s| s.as_str());

        // ツリー上の存在を確認（キャッシュ未ロードと存在しないを区別）
        let remote_absent = self.remote_tree.find_node_or_unloaded(Path::new(&path))
            == crate::tree::NodePresence::NotFound;
        let local_absent = self.local_tree.find_node_or_unloaded(Path::new(&path))
            == crate::tree::NodePresence::NotFound;

        self.current_diff = match (local_content, remote_content) {
            (Some(l), Some(r)) => {
                if engine::is_binary(l.as_bytes()) || engine::is_binary(r.as_bytes()) {
                    // テキストキャッシュ経由でバイナリ検出 → binary_cache に移動
                    let left = crate::diff::binary::BinaryInfo::from_bytes(l.as_bytes());
                    let right = crate::diff::binary::BinaryInfo::from_bytes(r.as_bytes());
                    self.local_binary_cache.insert(path.clone(), left.clone());
                    self.remote_binary_cache.insert(path.clone(), right.clone());
                    self.local_cache.remove(&path);
                    self.remote_cache.remove(&path);
                    Some(DiffResult::Binary {
                        left: Some(left),
                        right: Some(right),
                    })
                } else {
                    Some(engine::compute_diff(l, r))
                }
            }
            (Some(l), None) => {
                if remote_absent {
                    self.status_message = format!("{}: local only", path);
                } else {
                    self.status_message = format!("{}: remote content not loaded", path);
                }
                if engine::is_binary(l.as_bytes()) {
                    let info = crate::diff::binary::BinaryInfo::from_bytes(l.as_bytes());
                    self.local_binary_cache.insert(path.clone(), info.clone());
                    self.local_cache.remove(&path);
                    Some(DiffResult::Binary {
                        left: Some(info),
                        right: None,
                    })
                } else {
                    Some(engine::compute_diff(l, ""))
                }
            }
            (None, Some(r)) => {
                if local_absent {
                    self.status_message = format!("{}: remote only", path);
                } else {
                    self.status_message = format!("{}: local content not loaded", path);
                }
                if engine::is_binary(r.as_bytes()) {
                    let info = crate::diff::binary::BinaryInfo::from_bytes(r.as_bytes());
                    self.remote_binary_cache.insert(path.clone(), info.clone());
                    self.remote_cache.remove(&path);
                    Some(DiffResult::Binary {
                        left: None,
                        right: Some(info),
                    })
                } else {
                    Some(engine::compute_diff("", r))
                }
            }
            (None, None) => {
                self.status_message = format!("{}: content not loaded", path);
                None
            }
        };
        self.diff_scroll = 0;
        self.diff_cursor = 0;
        self.hunk_cursor = 0;
        self.pending_hunk_merge = None;

        // シンタックスハイライトキャッシュを構築
        if self.syntax_highlight_enabled {
            self.build_highlight_cache(&path);
        }
    }

    /// シンボリックリンク選択時の diff 計算
    fn select_symlink(&mut self, path: &str) {
        let local_target = self.symlink_target_from_tree(&self.local_tree, path);
        let remote_target = self.symlink_target_from_tree(&self.remote_tree, path);

        self.current_diff = Some(DiffResult::SymlinkDiff {
            left_target: local_target,
            right_target: remote_target,
        });
        self.diff_scroll = 0;
        self.diff_cursor = 0;
        self.hunk_cursor = 0;
        self.pending_hunk_merge = None;
        self.status_message = format!("{}: symlink", path);
    }

    /// ツリーからシンボリックリンクのターゲットパスを取得する
    fn symlink_target_from_tree(&self, tree: &crate::tree::FileTree, path: &str) -> Option<String> {
        use crate::tree::NodeKind;
        let node = tree.find_node(path)?;
        if let NodeKind::Symlink { ref target } = node.kind {
            Some(target.clone())
        } else {
            None
        }
    }

    /// 指定パスのシンタックスハイライトキャッシュを構築する
    fn build_highlight_cache(&mut self, path: &str) {
        if let Some(content) = self.local_cache.get(path) {
            if self.highlight_cache_local.get(path).is_none() {
                let highlighted = self.highlighter.highlight_file(path, content);
                self.highlight_cache_local
                    .insert(path.to_string(), highlighted);
            }
        }
        if let Some(content) = self.remote_cache.get(path) {
            if self.highlight_cache_remote.get(path).is_none() {
                let highlighted = self.highlighter.highlight_file(path, content);
                self.highlight_cache_remote
                    .insert(path.to_string(), highlighted);
            }
        }
    }

    /// ローカルツリーの遅延読み込み（ディレクトリ展開時）
    pub fn load_local_children(&mut self, path: &str) {
        let full_path = self.local_tree.root.join(path);
        let exclude = self.active_exclude_patterns();
        match crate::local::scan_dir(&full_path, &exclude) {
            Ok(children) => {
                if let Some(node) = self.local_tree.find_node_mut(std::path::Path::new(path)) {
                    node.children = Some(children);
                    node.sort_children();
                }
            }
            Err(e) => {
                self.status_message = format!("Local directory scan failed: {}", e);
            }
        }
    }

    /// ディレクトリのリフレッシュ（子ノードをクリア）
    pub fn refresh_directory(&mut self, path: &str) {
        if let Some(node) = self.local_tree.find_node_mut(std::path::Path::new(path)) {
            node.children = None;
        }
        if let Some(node) = self.remote_tree.find_node_mut(std::path::Path::new(path)) {
            node.children = None;
        }
        self.rebuild_flat_nodes();
        self.status_message = format!("{}: refreshed", path);
    }

    /// 現在カーソル位置のパスを返す
    pub fn current_path(&self) -> Option<String> {
        self.flat_nodes
            .get(self.tree_cursor)
            .map(|n| n.path.clone())
    }

    /// 現在カーソル位置がディレクトリかどうか
    pub fn current_is_dir(&self) -> bool {
        self.flat_nodes
            .get(self.tree_cursor)
            .is_some_and(|n| n.is_dir)
    }

    /// 指定パス群のキャッシュを無効化する。
    ///
    /// ディレクトリマージ前に古いキャッシュを破棄して最新の内容を再取得するために使う。
    /// `load_file_content` が単一ファイルで行うキャッシュ無効化と同じ方針。
    pub fn invalidate_cache_for_paths(&mut self, paths: &[String]) {
        for path in paths {
            self.local_cache.remove(path);
            self.remote_cache.remove(path);
            self.local_binary_cache.remove(path);
            self.remote_binary_cache.remove(path);
        }
    }

    /// コンテンツキャッシュをクリアする (r キー)
    pub fn clear_cache(&mut self) {
        self.local_cache.clear();
        self.remote_cache.clear();
        self.local_binary_cache.clear();
        self.remote_binary_cache.clear();
        self.error_paths.clear();
        self.highlight_cache_local.clear();
        self.highlight_cache_remote.clear();
        self.current_diff = None;
        self.selected_path = None;
        self.status_message = "Cache cleared".to_string();
    }

    /// 指定テーマを適用する。
    pub fn apply_theme(&mut self, name: &str) {
        let theme = crate::theme::load_theme(name);
        self.palette = crate::theme::TuiPalette::from_theme(&theme);
        self.highlighter.set_theme(theme);
        self.theme_name = name.to_string();
        // ハイライトキャッシュをクリア（テーマが変わると色が変わる）
        self.highlight_cache_local.clear();
        self.highlight_cache_remote.clear();
        // 現在のファイルのキャッシュを再構築
        if self.syntax_highlight_enabled {
            if let Some(path) = self.selected_path.clone() {
                self.build_highlight_cache(&path);
            }
        }
    }

    /// テーマを次のビルトインテーマに切り替える (T キー)
    pub fn cycle_theme(&mut self) {
        let next = crate::theme::next_theme_name(&self.theme_name);
        self.apply_theme(&next);
        // 永続化
        crate::state::save_state(&crate::state::PersistedState {
            theme: next.clone(),
        });
        self.status_message = format!("Theme: {}", next);
    }

    /// シンタックスハイライトの ON/OFF を切り替える (S キー)
    pub fn toggle_syntax_highlight(&mut self) {
        self.syntax_highlight_enabled = !self.syntax_highlight_enabled;
        if self.syntax_highlight_enabled {
            self.status_message = "Syntax highlight: ON".to_string();
            // 現在のファイルのキャッシュを構築
            if let Some(path) = self.selected_path.clone() {
                self.build_highlight_cache(&path);
            }
        } else {
            self.status_message = "Syntax highlight: OFF".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, FlatNode};
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_test_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_state_with_file(path: &str) -> AppState {
        let local_tree = make_test_tree(vec![FileNode::new_file(path)]);
        let remote_tree = make_test_tree(vec![FileNode::new_file(path)]);
        let mut state = AppState::new(
            local_tree,
            remote_tree,
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        // flat_nodes にファイルが入るように設定
        state.flat_nodes = vec![FlatNode {
            path: path.to_string(),
            name: path.to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
        }];
        state
    }

    #[test]
    fn test_select_file_both_cached_equal() {
        let mut state = make_state_with_file("a.rs");
        state
            .local_cache
            .insert("a.rs".to_string(), "hello".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "hello".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert_eq!(state.selected_path, Some("a.rs".to_string()));
        assert!(matches!(state.current_diff, Some(DiffResult::Equal)));
    }

    #[test]
    fn test_select_file_both_cached_modified() {
        let mut state = make_state_with_file("a.rs");
        state
            .local_cache
            .insert("a.rs".to_string(), "old".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "new".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(matches!(
            state.current_diff,
            Some(DiffResult::Modified { .. })
        ));
    }

    #[test]
    fn test_select_file_local_only() {
        let local_tree = make_test_tree(vec![FileNode::new_file("a.rs")]);
        let remote_tree = make_test_tree(vec![]);
        let mut state = AppState::new(
            local_tree,
            remote_tree,
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state.flat_nodes = vec![FlatNode {
            path: "a.rs".to_string(),
            name: "a.rs".to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
        }];
        state
            .local_cache
            .insert("a.rs".to_string(), "content".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.status_message.contains("local only"));
    }

    #[test]
    fn test_select_file_remote_only() {
        let local_tree = make_test_tree(vec![]);
        let remote_tree = make_test_tree(vec![FileNode::new_file("a.rs")]);
        let mut state = AppState::new(
            local_tree,
            remote_tree,
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state.flat_nodes = vec![FlatNode {
            path: "a.rs".to_string(),
            name: "a.rs".to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
        }];
        state
            .remote_cache
            .insert("a.rs".to_string(), "content".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.status_message.contains("remote only"));
    }

    #[test]
    fn test_select_file_neither_cached() {
        let mut state = make_state_with_file("a.rs");
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.status_message.contains("not loaded"));
        assert!(state.current_diff.is_none());
    }

    #[test]
    fn test_select_file_dir_skipped() {
        let mut state = make_state_with_file("src");
        state.flat_nodes[0].is_dir = true;
        state.tree_cursor = 0;
        let old_selected = state.selected_path.clone();
        state.select_file();
        assert_eq!(state.selected_path, old_selected);
    }

    #[test]
    fn test_select_file_out_of_bounds() {
        let mut state = make_state_with_file("a.rs");
        state.tree_cursor = 999;
        state.select_file();
        // 何も変わらない
        assert!(state.selected_path.is_none());
    }

    #[test]
    fn test_select_file_resets_scroll() {
        let mut state = make_state_with_file("a.rs");
        state
            .local_cache
            .insert("a.rs".to_string(), "x".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "x".to_string());
        state.diff_scroll = 10;
        state.diff_cursor = 5;
        state.hunk_cursor = 3;
        state.tree_cursor = 0;
        state.select_file();
        assert_eq!(state.diff_scroll, 0);
        assert_eq!(state.diff_cursor, 0);
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_current_path() {
        let state = make_state_with_file("a.rs");
        assert_eq!(state.current_path(), Some("a.rs".to_string()));
    }

    #[test]
    fn test_current_path_empty() {
        let state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.current_path(), None);
    }

    #[test]
    fn test_current_is_dir_false() {
        let state = make_state_with_file("a.rs");
        assert!(!state.current_is_dir());
    }

    #[test]
    fn test_current_is_dir_true() {
        let mut state = make_state_with_file("src");
        state.flat_nodes[0].is_dir = true;
        assert!(state.current_is_dir());
    }

    #[test]
    fn test_invalidate_cache_for_paths() {
        let mut state = make_state_with_file("a.rs");
        state
            .local_cache
            .insert("a.rs".to_string(), "x".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "y".to_string());
        state
            .local_cache
            .insert("b.rs".to_string(), "z".to_string());
        state.invalidate_cache_for_paths(&["a.rs".to_string()]);
        assert!(!state.local_cache.contains_key("a.rs"));
        assert!(!state.remote_cache.contains_key("a.rs"));
        assert!(state.local_cache.contains_key("b.rs"));
    }

    #[test]
    fn test_clear_cache() {
        let mut state = make_state_with_file("a.rs");
        state
            .local_cache
            .insert("a.rs".to_string(), "x".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "y".to_string());
        state.error_paths.insert("a.rs".to_string());
        state.selected_path = Some("a.rs".to_string());
        state.current_diff = Some(DiffResult::Equal);
        state.clear_cache();
        assert!(state.local_cache.is_empty());
        assert!(state.remote_cache.is_empty());
        assert!(state.error_paths.is_empty());
        assert!(state.selected_path.is_none());
        assert!(state.current_diff.is_none());
        assert!(state.status_message.contains("Cache cleared"));
    }

    #[test]
    fn test_toggle_syntax_highlight() {
        let mut state = make_state_with_file("a.rs");
        assert!(state.syntax_highlight_enabled);
        state.toggle_syntax_highlight();
        assert!(!state.syntax_highlight_enabled);
        assert!(state.status_message.contains("OFF"));
        state.toggle_syntax_highlight();
        assert!(state.syntax_highlight_enabled);
        assert!(state.status_message.contains("ON"));
    }

    #[test]
    fn test_select_file_binary_content() {
        let mut state = make_state_with_file("img.png");
        // バイナリ判定: NULバイトを含む
        state
            .local_cache
            .insert("img.png".to_string(), "hello\x00world".to_string());
        state
            .remote_cache
            .insert("img.png".to_string(), "hello\x00world".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(matches!(
            state.current_diff,
            Some(DiffResult::Binary { .. })
        ));
    }

    #[test]
    fn test_select_file_clears_pending_hunk() {
        let mut state = make_state_with_file("a.rs");
        state
            .local_cache
            .insert("a.rs".to_string(), "x".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "x".to_string());
        state.pending_hunk_merge = Some(crate::diff::engine::HunkDirection::RightToLeft);
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.pending_hunk_merge.is_none());
    }
}
