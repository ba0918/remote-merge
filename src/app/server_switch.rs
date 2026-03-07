//! サーバ切替時の状態リセット処理。
//!
//! 右側サーバの変更に伴うツリー・キャッシュ・diff 状態のリセットを行う。

use crate::tree::FileTree;

use super::AppState;

impl AppState {
    /// サーバ切替後にツリーを再構築する
    pub fn switch_server(&mut self, new_server: String, remote_tree: FileTree) {
        self.server_name = new_server.clone();
        self.right_source = super::Side::Remote(new_server);
        let label = super::side::comparison_label(&self.left_source, &self.right_source);
        self.status_message = format!("{} | Tab: switch focus | q: quit", label);
        self.right_tree = remote_tree;
        self.reset_diff_state();
        self.is_connected = true;
    }

    /// ペア切り替え（LEFT/RIGHT 両方を入れ替え）
    ///
    /// available_servers から left/right を除いた最初のサーバを ref_source に自動設定。
    pub fn switch_pair(
        &mut self,
        new_left: super::Side,
        new_right: super::Side,
        left_tree: FileTree,
        right_tree: FileTree,
    ) {
        self.left_source = new_left;
        self.right_source = new_right.clone();
        self.server_name = new_right.display_name().to_string();
        let label = super::side::comparison_label(&self.left_source, &self.right_source);
        self.status_message = format!("{} | Tab: switch focus | q: quit", label);
        self.left_tree = left_tree;
        self.right_tree = right_tree;
        self.reset_diff_state();
        self.is_connected = true;

        // reference サーバを自動計算（left/right 以外の先頭）
        self.auto_select_reference();
    }

    /// diff 状態をリセットする共通ヘルパー
    fn reset_diff_state(&mut self) {
        self.clear_all_content_caches();
        self.current_diff = None;
        self.selected_path = None;
        self.diff_scroll = 0;
        self.diff_cursor = 0;
        self.undo_stack.clear();
        self.showing_ref_diff = false;
        self.clear_scan_cache();
        self.rebuild_flat_nodes();
    }

    /// available_servers + "local" から left/right を除いた最初のサーバを reference に自動選択
    fn auto_select_reference(&mut self) {
        let left_name = self.left_source.display_name().to_string();
        let right_name = self.right_source.display_name().to_string();

        // available_servers は config.servers.keys() なので "local" を含まない。
        // "local" を先頭に追加して候補とする。
        let mut candidates = vec!["local".to_string()];
        for s in &self.available_servers {
            if s != "local" {
                candidates.push(s.clone());
            }
        }

        let ref_server = candidates
            .iter()
            .find(|s| {
                let name = s.as_str();
                name != left_name && name != right_name
            })
            .cloned();

        if ref_server.is_some() {
            // ref_tree は遅延取得のため、ここではソースのみ設定
            // ref_tree は execute_ref_connect() で接続 + 取得される
            self.ref_source = ref_server.map(|name| {
                if name == "local" {
                    super::Side::Local
                } else {
                    super::Side::Remote(name)
                }
            });
            self.ref_tree = None;
            self.ref_cache.clear();
            self.ref_binary_cache.clear();
        } else {
            self.clear_reference();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::{Badge, CacheSnapshot};
    use crate::app::Side;
    use crate::tree::{FileNode, FileTree};
    use std::path::PathBuf;

    fn make_test_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    fn make_state() -> AppState {
        AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        )
    }

    #[test]
    fn test_switch_server_basic() {
        let mut state = make_state();
        state
            .right_cache
            .insert("a.txt".to_string(), "old".to_string());
        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);
        assert_eq!(state.server_name, "staging");
        assert!(state.right_cache.is_empty());
        assert!(state.is_connected);
    }

    #[test]
    fn test_switch_server_clears_local_cache() {
        let mut state = make_state();
        state
            .left_cache
            .insert("a.txt".to_string(), "old content".to_string());
        state
            .right_cache
            .insert("a.txt".to_string(), "remote content".to_string());
        state.error_paths.insert("a.txt".to_string());
        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);
        assert!(state.left_cache.is_empty());
        assert!(state.right_cache.is_empty());
        assert!(state.error_paths.is_empty());
    }

    #[test]
    fn test_switch_server_clears_scan_cache_and_filter_mode() {
        let mut state = make_state();
        state.scan_left_tree = Some(vec![FileNode::new_file("a.txt")]);
        state.scan_right_tree = Some(vec![FileNode::new_file("a.txt")]);
        state.scan_statuses = Some(std::collections::HashMap::new());
        state.diff_filter_mode = true;
        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);
        assert!(state.scan_left_tree.is_none());
        assert!(state.scan_right_tree.is_none());
        assert!(state.scan_statuses.is_none());
        assert!(!state.diff_filter_mode);
    }

    #[test]
    fn test_switch_server_clears_undo_stack() {
        let mut state = make_state();
        state.undo_stack.push_back(CacheSnapshot {
            local_content: "old".to_string(),
            remote_content: "old-remote".to_string(),
            diff: None,
        });
        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);
        assert!(state.undo_stack.is_empty());
        assert!(!state.has_unsaved_changes());
    }

    #[test]
    fn test_switch_server_badge_uses_new_tree() {
        let mut state = make_state();
        let badge_before = state.compute_badge("a.txt", false);
        assert_eq!(badge_before, Badge::Unchecked);
        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);
        let badge_after = state.compute_badge("a.txt", false);
        assert_eq!(badge_after, Badge::LeftOnly);
    }

    #[test]
    fn test_diff_filter_to_server_switch_restores_all_files() {
        let local_nodes = vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ];
        let remote_nodes = vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("same.txt".to_string(), "identical".to_string());
        state
            .right_cache
            .insert("same.txt".to_string(), "identical".to_string());
        state
            .left_cache
            .insert("changed.txt".to_string(), "local ver".to_string());
        state
            .right_cache
            .insert("changed.txt".to_string(), "remote ver".to_string());
        let statuses = std::collections::HashMap::from([
            (
                "changed.txt".to_string(),
                crate::service::types::FileStatusKind::Modified,
            ),
            (
                "same.txt".to_string(),
                crate::service::types::FileStatusKind::Equal,
            ),
        ]);
        state.set_scan_result(
            vec![
                FileNode::new_file("changed.txt"),
                FileNode::new_file("same.txt"),
            ],
            vec![
                FileNode::new_file("changed.txt"),
                FileNode::new_file("same.txt"),
            ],
            statuses,
        );
        state.toggle_diff_filter();
        assert!(state.diff_filter_mode);
        let filtered_count = state.flat_nodes.iter().filter(|n| !n.is_dir).count();
        let new_tree = make_test_tree(vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ]);
        state.switch_server("staging".to_string(), new_tree);
        assert!(!state.diff_filter_mode);
        let all_count = state.flat_nodes.iter().filter(|n| !n.is_dir).count();
        assert!(all_count >= filtered_count);
    }

    // ── switch_pair ──

    #[test]
    fn test_switch_pair_basic() {
        let mut state = make_state();
        state.available_servers = vec![
            "develop".to_string(),
            "staging".to_string(),
            "release".to_string(),
        ];
        let new_left = make_test_tree(vec![FileNode::new_file("left.txt")]);
        let new_right = make_test_tree(vec![FileNode::new_file("right.txt")]);
        state.switch_pair(
            Side::Remote("staging".to_string()),
            Side::Remote("release".to_string()),
            new_left,
            new_right,
        );
        assert_eq!(state.left_source, Side::Remote("staging".to_string()));
        assert_eq!(state.right_source, Side::Remote("release".to_string()));
        assert!(state.is_connected);
        assert!(state.left_cache.is_empty());
        assert!(state.right_cache.is_empty());
    }

    #[test]
    fn test_switch_pair_clears_caches() {
        let mut state = make_state();
        state.available_servers = vec!["develop".to_string(), "staging".to_string()];
        state
            .left_cache
            .insert("a.txt".to_string(), "x".to_string());
        state
            .right_cache
            .insert("a.txt".to_string(), "y".to_string());
        state.undo_stack.push_back(CacheSnapshot {
            local_content: "old".to_string(),
            remote_content: "old".to_string(),
            diff: None,
        });
        state.switch_pair(
            Side::Local,
            Side::Remote("staging".to_string()),
            make_test_tree(vec![]),
            make_test_tree(vec![]),
        );
        assert!(state.left_cache.is_empty());
        assert!(state.right_cache.is_empty());
        assert!(state.undo_stack.is_empty());
    }

    #[test]
    fn test_switch_pair_auto_selects_reference() {
        let mut state = make_state();
        state.available_servers = vec![
            "develop".to_string(),
            "staging".to_string(),
            "release".to_string(),
        ];
        state.switch_pair(
            Side::Local,
            Side::Remote("develop".to_string()),
            make_test_tree(vec![]),
            make_test_tree(vec![]),
        );
        // local と develop 以外 → staging が reference に
        assert_eq!(state.ref_source, Some(Side::Remote("staging".to_string())));
        // ref_tree はまだ None（遅延取得）
        assert!(state.ref_tree.is_none());
    }

    #[test]
    fn test_switch_pair_no_reference_when_only_two_servers() {
        let mut state = make_state();
        state.available_servers = vec!["develop".to_string()];
        state.switch_pair(
            Side::Local,
            Side::Remote("develop".to_string()),
            make_test_tree(vec![]),
            make_test_tree(vec![]),
        );
        assert!(state.ref_source.is_none());
    }

    #[test]
    fn test_switch_pair_ref_recalculated_when_old_ref_becomes_pair() {
        let mut state = make_state();
        state.available_servers = vec![
            "develop".to_string(),
            "staging".to_string(),
            "release".to_string(),
        ];
        // 最初のペア: local <-> develop → ref = staging
        state.switch_pair(
            Side::Local,
            Side::Remote("develop".to_string()),
            make_test_tree(vec![]),
            make_test_tree(vec![]),
        );
        assert_eq!(state.ref_source, Some(Side::Remote("staging".to_string())));

        // ペア変更: local <-> staging → ref = develop
        state.switch_pair(
            Side::Local,
            Side::Remote("staging".to_string()),
            make_test_tree(vec![]),
            make_test_tree(vec![]),
        );
        assert_eq!(state.ref_source, Some(Side::Remote("develop".to_string())));
    }

    #[test]
    fn test_switch_pair_remote_remote_selects_local_as_ref() {
        let mut state = make_state();
        state.available_servers = vec!["develop".to_string(), "staging".to_string()];
        // develop <-> staging → "local" が候補に入り reference に
        state.switch_pair(
            Side::Remote("develop".to_string()),
            Side::Remote("staging".to_string()),
            make_test_tree(vec![]),
            make_test_tree(vec![]),
        );
        assert_eq!(state.ref_source, Some(Side::Local));
    }

    #[test]
    fn test_switch_pair_remote_remote_with_three_servers() {
        let mut state = make_state();
        state.available_servers = vec![
            "develop".to_string(),
            "staging".to_string(),
            "release".to_string(),
        ];
        // develop <-> staging → "local" が候補先頭なので reference に
        state.switch_pair(
            Side::Remote("develop".to_string()),
            Side::Remote("staging".to_string()),
            make_test_tree(vec![]),
            make_test_tree(vec![]),
        );
        assert_eq!(state.ref_source, Some(Side::Local));
    }
}
