//! ファイル選択・読み込み・キャッシュ管理。

use std::path::Path;

use crate::diff::engine::{self, DiffResult};

use super::AppState;

impl AppState {
    /// diff ビューの状態（スクロール・カーソル・ハンク）をリセットする
    pub fn reset_diff_view_state(&mut self) {
        self.diff_scroll = 0;
        self.diff_cursor = 0;
        self.hunk_cursor = 0;
        self.pending_hunk_merge = None;
    }

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
        let local_bin = self.left_binary_cache.get(&path).cloned();
        let remote_bin = self.right_binary_cache.get(&path).cloned();
        if local_bin.is_some() || remote_bin.is_some() {
            self.current_diff = Some(DiffResult::Binary {
                left: local_bin,
                right: remote_bin,
            });
            self.reset_diff_view_state();
            return;
        }

        // テキストキャッシュからコンテンツを取得して diff
        let local_content = self.left_cache.get(&path).map(|s| s.as_str());
        let remote_content = self.right_cache.get(&path).map(|s| s.as_str());

        // ツリー上の存在を確認（キャッシュ未ロードと存在しないを区別）
        let remote_absent = self.right_tree.find_node_or_unloaded(Path::new(&path))
            == crate::tree::NodePresence::NotFound;
        let local_absent = self.left_tree.find_node_or_unloaded(Path::new(&path))
            == crate::tree::NodePresence::NotFound;

        self.current_diff = match (local_content, remote_content) {
            (Some(l), Some(r)) => {
                if engine::is_binary(l.as_bytes()) || engine::is_binary(r.as_bytes()) {
                    // テキストキャッシュ経由でバイナリ検出 → binary_cache に移動
                    let left = crate::diff::binary::BinaryInfo::from_bytes(l.as_bytes());
                    let right = crate::diff::binary::BinaryInfo::from_bytes(r.as_bytes());
                    self.left_binary_cache.insert(path.clone(), left.clone());
                    self.right_binary_cache.insert(path.clone(), right.clone());
                    self.left_cache.remove(&path);
                    self.right_cache.remove(&path);
                    Some(DiffResult::Binary {
                        left: Some(left),
                        right: Some(right),
                    })
                } else {
                    let diff = engine::compute_diff(l, r);
                    // Equal + ref 差分がある場合、ref diff を自動表示
                    if matches!(&diff, DiffResult::Equal) && self.has_reference() {
                        if let Some(ref_content) = self.ref_cache.get(&path).cloned() {
                            let ref_diff = engine::compute_diff(l, &ref_content);
                            if matches!(&ref_diff, DiffResult::Equal) {
                                self.showing_ref_diff = false;
                                Some(diff)
                            } else {
                                self.showing_ref_diff = true;
                                Some(ref_diff)
                            }
                        } else {
                            self.showing_ref_diff = false;
                            Some(diff)
                        }
                    } else {
                        self.showing_ref_diff = false;
                        Some(diff)
                    }
                }
            }
            (Some(l), None) => {
                self.status_message = Self::one_sided_status(&path, remote_absent, "local");
                if engine::is_binary(l.as_bytes()) {
                    let info = crate::diff::binary::BinaryInfo::from_bytes(l.as_bytes());
                    self.left_binary_cache.insert(path.clone(), info.clone());
                    self.left_cache.remove(&path);
                    Some(DiffResult::Binary {
                        left: Some(info),
                        right: None,
                    })
                } else {
                    Some(engine::compute_diff(l, ""))
                }
            }
            (None, Some(r)) => {
                self.status_message = Self::one_sided_status(&path, local_absent, "remote");
                if engine::is_binary(r.as_bytes()) {
                    let info = crate::diff::binary::BinaryInfo::from_bytes(r.as_bytes());
                    self.right_binary_cache.insert(path.clone(), info.clone());
                    self.right_cache.remove(&path);
                    Some(DiffResult::Binary {
                        left: None,
                        right: Some(info),
                    })
                } else {
                    Some(engine::compute_diff("", r))
                }
            }
            (None, None) => {
                // ref-only ファイル: ref_cache にコンテンツがあれば表示
                if let Some(ref_content) = self.ref_cache.get(&path).cloned() {
                    self.showing_ref_diff = true;
                    self.status_message = format!("{}: ref only", path);
                    Some(engine::compute_diff("", &ref_content))
                } else {
                    self.status_message = format!("{}: content not loaded", path);
                    None
                }
            }
        };
        self.reset_diff_view_state();

        // flat_nodes のバッジを更新（Unchecked → Equal/Modified 等）
        let new_badge = self.compute_badge(&path, false);
        if let Some(node) = self.flat_nodes.get_mut(self.tree_cursor) {
            node.badge = new_badge;
        }

        // シンタックスハイライトキャッシュを構築
        if self.syntax_highlight_enabled {
            self.build_highlight_cache(&path);
        }
    }

    /// シンボリックリンク選択時の diff 計算
    fn select_symlink(&mut self, path: &str) {
        let local_target = self.symlink_target_from_tree(&self.left_tree, path);
        let remote_target = self.symlink_target_from_tree(&self.right_tree, path);

        self.current_diff = Some(DiffResult::SymlinkDiff {
            left_target: local_target,
            right_target: remote_target,
        });
        self.reset_diff_view_state();
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

    /// ファイル選択時のステータスメッセージを決定する（片側のみロード済みの場合）。
    ///
    /// ツリー上にノードが存在しない場合は "local/remote only"、
    /// 存在するがキャッシュがない場合は "content not loaded" を返す。
    fn one_sided_status(path: &str, absent: bool, side: &str) -> String {
        if absent {
            format!("{}: {} only", path, side)
        } else {
            format!("{}: {} content not loaded", path, side)
        }
    }

    /// 指定パスのシンタックスハイライトキャッシュを構築する
    pub(super) fn build_highlight_cache(&mut self, path: &str) {
        if let Some(content) = self.left_cache.get(path) {
            if self.highlight_cache_left.get(path).is_none() {
                let highlighted = self.highlighter.highlight_file(path, content);
                self.highlight_cache_left
                    .insert(path.to_string(), highlighted);
            }
        }
        if let Some(content) = self.right_cache.get(path) {
            if self.highlight_cache_right.get(path).is_none() {
                let highlighted = self.highlighter.highlight_file(path, content);
                self.highlight_cache_right
                    .insert(path.to_string(), highlighted);
            }
        }
    }

    /// ローカルツリーの遅延読み込み（ディレクトリ展開時）
    pub fn load_local_children(&mut self, path: &str) {
        let full_path = self.left_tree.root.join(path);
        let exclude = self.active_exclude_patterns();
        match crate::local::scan_dir(&full_path, &exclude, path) {
            Ok(children) => {
                let filtered = crate::filter::filter_children_by_include(
                    children,
                    path,
                    &self.include_patterns,
                );
                self.apply_local_children(path, filtered);
            }
            Err(e) => {
                self.status_message = format!("Local directory scan failed: {}", e);
            }
        }
    }

    /// スキャン結果をローカルツリーに適用する（純粋な状態更新）
    pub fn apply_local_children(&mut self, path: &str, children: Vec<crate::tree::FileNode>) {
        if let Some(node) = self.left_tree.find_node_mut(std::path::Path::new(path)) {
            node.children = Some(children);
            node.sort_children();
        }
    }

    /// ローカルツリーの全ディレクトリを再帰的にロードする（検索用）。
    ///
    /// 未ロード（`children = None`）のディレクトリをスキャンし、
    /// 検索が全ファイルを対象にできるようにする。
    pub fn load_local_tree_recursive(&mut self) {
        let exclude = self.active_exclude_patterns();
        let include = self.include_patterns.clone();
        let root = self.left_tree.root.clone();
        Self::load_children_recursive(&mut self.left_tree.nodes, &root, &exclude, &include, "");
    }

    /// FileNode リストの未ロードディレクトリを再帰的にロードする
    fn load_children_recursive(
        nodes: &mut [crate::tree::FileNode],
        base_path: &std::path::Path,
        exclude: &[String],
        include: &[String],
        parent_rel: &str,
    ) {
        for node in nodes.iter_mut() {
            if !node.is_dir() {
                continue;
            }
            let dir_path = base_path.join(&node.name);
            let rel = if parent_rel.is_empty() {
                node.name.clone()
            } else {
                format!("{}/{}", parent_rel, node.name)
            };
            if !node.is_loaded() {
                if let Ok(children) = crate::local::scan_dir(&dir_path, exclude, &rel) {
                    node.children = Some(crate::filter::filter_children_by_include(
                        children, &rel, include,
                    ));
                    node.sort_children();
                }
            }
            if let Some(ref mut children) = node.children {
                Self::load_children_recursive(children, &dir_path, exclude, include, &rel);
            }
        }
    }

    /// ディレクトリのリフレッシュ（子ノードをクリア）
    pub fn refresh_directory(&mut self, path: &str) {
        if let Some(node) = self.left_tree.find_node_mut(std::path::Path::new(path)) {
            node.children = None;
        }
        if let Some(node) = self.right_tree.find_node_mut(std::path::Path::new(path)) {
            node.children = None;
        }
        // ref_tree の子もクリアして stale data を防ぐ
        if let Some(ref mut tree) = self.ref_tree {
            if let Some(node) = tree.find_node_mut(std::path::Path::new(path)) {
                node.children = None;
            }
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
            self.left_cache.remove(path);
            self.right_cache.remove(path);
            self.left_binary_cache.remove(path);
            self.right_binary_cache.remove(path);
            self.conflict_cache.remove(path);
        }
    }

    /// 全コンテンツキャッシュ（テキスト + バイナリ + エラー）をクリアする
    pub fn clear_all_content_caches(&mut self) {
        self.left_cache.clear();
        self.right_cache.clear();
        self.left_binary_cache.clear();
        self.right_binary_cache.clear();
        self.error_paths.clear();
        self.conflict_cache.clear();
    }

    /// コンテンツキャッシュをクリアする (r キー)
    pub fn clear_cache(&mut self) {
        self.clear_all_content_caches();
        self.highlight_cache_left.clear();
        self.highlight_cache_right.clear();
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
        self.highlight_cache_left.clear();
        self.highlight_cache_right.clear();
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
    use crate::app::Side;
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
            Side::Local,
            Side::Remote("develop".to_string()),
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
            ref_only: false,
        }];
        state
    }

    #[test]
    fn test_select_file_both_cached_equal() {
        let mut state = make_state_with_file("a.rs");
        state
            .left_cache
            .insert("a.rs".to_string(), "hello".to_string());
        state
            .right_cache
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
            .left_cache
            .insert("a.rs".to_string(), "old".to_string());
        state
            .right_cache
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
            Side::Local,
            Side::Remote("develop".to_string()),
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
            ref_only: false,
        }];
        state
            .left_cache
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
            Side::Local,
            Side::Remote("develop".to_string()),
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
            ref_only: false,
        }];
        state
            .right_cache
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
        state.left_cache.insert("a.rs".to_string(), "x".to_string());
        state
            .right_cache
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
            Side::Local,
            Side::Remote("develop".to_string()),
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
        state.left_cache.insert("a.rs".to_string(), "x".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "y".to_string());
        state.left_cache.insert("b.rs".to_string(), "z".to_string());
        state.invalidate_cache_for_paths(&["a.rs".to_string()]);
        assert!(!state.left_cache.contains_key("a.rs"));
        assert!(!state.right_cache.contains_key("a.rs"));
        assert!(state.left_cache.contains_key("b.rs"));
    }

    #[test]
    fn test_clear_cache() {
        let mut state = make_state_with_file("a.rs");
        state.left_cache.insert("a.rs".to_string(), "x".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "y".to_string());
        state.error_paths.insert("a.rs".to_string());
        state.selected_path = Some("a.rs".to_string());
        state.current_diff = Some(DiffResult::Equal);
        state.clear_cache();
        assert!(state.left_cache.is_empty());
        assert!(state.right_cache.is_empty());
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
            .left_cache
            .insert("img.png".to_string(), "hello\x00world".to_string());
        state
            .right_cache
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
        state.left_cache.insert("a.rs".to_string(), "x".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "x".to_string());
        state.pending_hunk_merge = Some(crate::diff::engine::HunkDirection::RightToLeft);
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.pending_hunk_merge.is_none());
    }

    #[test]
    fn test_clear_cache_also_clears_binary_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.left_binary_cache.insert(
            "x.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 1,
                sha256: "a".to_string(),
            },
        );
        state.right_binary_cache.insert(
            "x.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 1,
                sha256: "a".to_string(),
            },
        );
        state.clear_cache();
        assert!(state.left_binary_cache.is_empty());
        assert!(state.right_binary_cache.is_empty());
    }

    #[test]
    fn test_select_file_updates_badge_equal() {
        let mut state = make_state_with_file("a.rs");
        assert_eq!(state.flat_nodes[0].badge, Badge::Unchecked);
        state
            .left_cache
            .insert("a.rs".to_string(), "hello".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "hello".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert_eq!(state.flat_nodes[0].badge, Badge::Equal);
    }

    #[test]
    fn test_select_file_updates_badge_modified() {
        let mut state = make_state_with_file("a.rs");
        assert_eq!(state.flat_nodes[0].badge, Badge::Unchecked);
        state
            .left_cache
            .insert("a.rs".to_string(), "old".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "new".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert_eq!(state.flat_nodes[0].badge, Badge::Modified);
    }

    // ── showing_ref_diff テスト ──

    fn make_state_with_ref(path: &str) -> AppState {
        let mut state = make_state_with_file(path);
        state.set_reference(
            Side::Remote("staging".to_string()),
            make_test_tree(vec![FileNode::new_file(path)]),
        );
        state
    }

    #[test]
    fn test_equal_with_ref_diff_shows_ref_diff() {
        let mut state = make_state_with_ref("a.rs");
        state
            .left_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .ref_cache
            .insert("a.rs".to_string(), "different".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.showing_ref_diff);
        assert!(matches!(
            state.current_diff,
            Some(DiffResult::Modified { .. })
        ));
    }

    #[test]
    fn test_equal_with_ref_also_equal() {
        let mut state = make_state_with_ref("a.rs");
        state
            .left_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .ref_cache
            .insert("a.rs".to_string(), "same".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(!state.showing_ref_diff);
        assert!(matches!(state.current_diff, Some(DiffResult::Equal)));
    }

    #[test]
    fn test_equal_with_no_ref_cache() {
        let mut state = make_state_with_ref("a.rs");
        state
            .left_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "same".to_string());
        // ref_cache にコンテンツなし
        state.tree_cursor = 0;
        state.select_file();
        assert!(!state.showing_ref_diff);
        assert!(matches!(state.current_diff, Some(DiffResult::Equal)));
    }

    #[test]
    fn test_equal_without_reference_server() {
        let mut state = make_state_with_file("a.rs");
        // ref なし
        state
            .left_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "same".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(!state.showing_ref_diff);
    }

    #[test]
    fn test_modified_resets_showing_ref_diff() {
        let mut state = make_state_with_ref("a.rs");
        state.showing_ref_diff = true; // 前のファイルで設定されていた
        state
            .left_cache
            .insert("a.rs".to_string(), "old".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "new".to_string());
        state
            .ref_cache
            .insert("a.rs".to_string(), "ref".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(!state.showing_ref_diff);
    }

    #[test]
    fn test_file_switch_resets_showing_ref_diff() {
        let mut state = make_state_with_ref("a.rs");
        state.showing_ref_diff = true;
        // 新しいファイルで Modified
        state.left_cache.insert("a.rs".to_string(), "x".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "y".to_string());
        state.tree_cursor = 0;
        state.select_file();
        assert!(!state.showing_ref_diff);
    }

    // ── ref-only ファイルテスト ──

    #[test]
    fn test_ref_only_file_shows_ref_content_as_diff() {
        // ref-only ファイル: left/right にコンテンツなし、ref_cache にだけある
        let mut state = make_state_with_ref("staging_config.rs");
        state.flat_nodes = vec![FlatNode {
            path: "staging_config.rs".to_string(),
            name: "staging_config.rs".to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
            ref_only: true,
        }];
        state
            .ref_cache
            .insert("staging_config.rs".to_string(), "ref content".to_string());
        // left/right にはコンテンツなし
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.showing_ref_diff);
        assert!(state.current_diff.is_some());
        assert!(
            state.status_message.contains("ref only"),
            "status should indicate ref only, got: {}",
            state.status_message
        );
    }

    #[test]
    fn test_ref_only_file_no_ref_cache_shows_not_loaded() {
        // ref-only ファイルだが ref_cache にもコンテンツがない
        let mut state = make_state_with_ref("staging_config.rs");
        state.flat_nodes = vec![FlatNode {
            path: "staging_config.rs".to_string(),
            name: "staging_config.rs".to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Unchecked,
            ref_only: true,
        }];
        state.tree_cursor = 0;
        state.select_file();
        assert!(state.current_diff.is_none());
        assert!(state.status_message.contains("not loaded"));
    }

    #[test]
    fn test_refresh_directory_clears_ref_tree_children() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
            make_test_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.set_reference(
            Side::Remote("staging".to_string()),
            make_test_tree(vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("a.rs")],
            )]),
        );

        // 全ツリーの src が展開済みであることを確認
        assert!(state.left_tree.find_node("src").unwrap().is_loaded());
        assert!(state.right_tree.find_node("src").unwrap().is_loaded());
        assert!(state
            .ref_tree
            .as_ref()
            .unwrap()
            .find_node("src")
            .unwrap()
            .is_loaded());

        // refresh_directory で全ツリーの src がクリアされる
        state.refresh_directory("src");

        assert!(!state.left_tree.find_node("src").unwrap().is_loaded());
        assert!(!state.right_tree.find_node("src").unwrap().is_loaded());
        assert!(
            !state
                .ref_tree
                .as_ref()
                .unwrap()
                .find_node("src")
                .unwrap()
                .is_loaded(),
            "ref_tree children should also be cleared by refresh_directory"
        );
    }

    // ── one_sided_status ──

    #[test]
    fn test_one_sided_status_absent_local() {
        let msg = AppState::one_sided_status("a.rs", true, "local");
        assert_eq!(msg, "a.rs: local only");
    }

    #[test]
    fn test_one_sided_status_absent_remote() {
        let msg = AppState::one_sided_status("a.rs", true, "remote");
        assert_eq!(msg, "a.rs: remote only");
    }

    #[test]
    fn test_one_sided_status_not_loaded_local() {
        let msg = AppState::one_sided_status("a.rs", false, "local");
        assert_eq!(msg, "a.rs: local content not loaded");
    }

    #[test]
    fn test_one_sided_status_not_loaded_remote() {
        let msg = AppState::one_sided_status("a.rs", false, "remote");
        assert_eq!(msg, "a.rs: remote content not loaded");
    }

    // ── apply_local_children ──

    #[test]
    fn test_apply_local_children_sets_children() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_dir("src")]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert!(
            !state.left_tree.find_node("src").unwrap().is_loaded(),
            "src should be unloaded initially"
        );

        let children = vec![FileNode::new_file("b.rs"), FileNode::new_file("a.rs")];
        state.apply_local_children("src", children);

        let node = state.left_tree.find_node("src").unwrap();
        assert!(node.is_loaded());
        let child_names: Vec<&str> = node
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        // sort_children でソートされている
        assert_eq!(child_names, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn test_apply_local_children_nonexistent_path() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        // 存在しないパスに適用してもパニックしない
        state.apply_local_children("nonexistent", vec![FileNode::new_file("a.rs")]);
    }

    // ── load_local_children + include フィルター ──

    #[test]
    fn test_load_local_children_applies_include_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(src.join("app")).unwrap();
        std::fs::create_dir(src.join("handler")).unwrap();
        std::fs::create_dir(src.join("ui")).unwrap();
        std::fs::write(src.join("main.rs"), "fn main(){}").unwrap();

        let mut state = AppState::new(
            FileTree::new(tmp.path()),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.include_patterns = vec!["src/app".to_string(), "src/handler".to_string()];

        // src を左ツリーに追加（未ロード状態）
        state.left_tree.nodes = vec![FileNode::new_dir("src")];

        state.load_local_children("src");

        let node = state.left_tree.find_node("src").unwrap();
        assert!(node.is_loaded());
        let child_names: Vec<&str> = node
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert!(child_names.contains(&"app"), "app should be included");
        assert!(
            child_names.contains(&"handler"),
            "handler should be included"
        );
        assert!(
            !child_names.contains(&"ui"),
            "ui should be excluded by include filter"
        );
        assert!(
            !child_names.contains(&"main.rs"),
            "main.rs should be excluded by include filter"
        );
    }

    #[test]
    fn test_load_local_children_no_include_returns_all() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(src.join("app")).unwrap();
        std::fs::create_dir(src.join("ui")).unwrap();
        std::fs::write(src.join("main.rs"), "fn main(){}").unwrap();

        let mut state = AppState::new(
            FileTree::new(tmp.path()),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        // include 空 → フィルタなし
        state.include_patterns = vec![];
        state.left_tree.nodes = vec![FileNode::new_dir("src")];

        state.load_local_children("src");

        let node = state.left_tree.find_node("src").unwrap();
        let child_names: Vec<&str> = node
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(child_names.len(), 3, "all children should be returned");
    }

    #[test]
    fn test_load_local_tree_recursive_applies_include_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let app = src.join("app");
        let ui = src.join("ui");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&ui).unwrap();
        std::fs::write(app.join("mod.rs"), "").unwrap();
        std::fs::write(ui.join("render.rs"), "").unwrap();

        let mut state = AppState::new(
            FileTree::new(tmp.path()),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.include_patterns = vec!["src/app".to_string()];
        // ルートに src（未ロード）のみ（bootstrap で include フィルタ済みの状態を再現）
        state.left_tree.nodes = vec![FileNode::new_dir("src")];

        state.load_local_tree_recursive();

        // src 配下: app のみ（ui は include 外）
        let src_node = state.left_tree.find_node("src").unwrap();
        let child_names: Vec<&str> = src_node
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert!(child_names.contains(&"app"), "app should be included");
        assert!(
            !child_names.contains(&"ui"),
            "ui should be excluded by include filter"
        );
    }
}
