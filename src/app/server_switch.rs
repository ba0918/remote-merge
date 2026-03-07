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
        self.clear_all_content_caches();
        self.current_diff = None;
        self.selected_path = None;
        self.diff_scroll = 0;
        self.diff_cursor = 0;
        self.undo_stack.clear();
        self.clear_scan_cache();
        self.rebuild_flat_nodes();
        self.is_connected = true;
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
}
