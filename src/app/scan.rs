//! 走査状態・フィルターモード管理。

use crate::tree::FileNode;

use super::types::ScanState;
use super::AppState;

impl AppState {
    /// フィルターモードの切り替え
    pub fn toggle_diff_filter(&mut self) {
        self.diff_filter_mode = !self.diff_filter_mode;
        self.rebuild_flat_nodes();
        if self.diff_filter_mode {
            let diff_count = self.flat_nodes.iter().filter(|n| !n.is_dir).count();
            self.status_message = format!("[DIFF ONLY] changes: {} files", diff_count);
        } else {
            self.status_message = "Normal view restored".to_string();
        }
    }

    /// 走査完了時にキャッシュを設定
    pub fn set_scan_result(&mut self, local_nodes: Vec<FileNode>, remote_nodes: Vec<FileNode>) {
        self.scan_left_tree = Some(local_nodes);
        self.scan_right_tree = Some(remote_nodes);
        self.scan_state = ScanState::Idle;
    }

    /// 走査キャッシュをクリア
    pub fn clear_scan_cache(&mut self) {
        self.scan_left_tree = None;
        self.scan_right_tree = None;
        self.scan_state = ScanState::Idle;
        if self.diff_filter_mode {
            self.diff_filter_mode = false;
            self.rebuild_flat_nodes();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        )
    }

    #[test]
    fn test_scan_state_default() {
        let state = make_state();
        assert!(matches!(state.scan_state, ScanState::Idle));
        assert!(!state.diff_filter_mode);
        assert!(state.scan_left_tree.is_none());
    }

    #[test]
    fn test_clear_scan_cache() {
        let mut state = make_state();
        state.scan_left_tree = Some(vec![]);
        state.scan_right_tree = Some(vec![]);
        state.diff_filter_mode = true;
        state.clear_scan_cache();
        assert!(state.scan_left_tree.is_none());
        assert!(state.scan_right_tree.is_none());
        assert!(!state.diff_filter_mode);
    }

    #[test]
    fn test_clear_scan_cache_disables_filter_mode() {
        let local_nodes = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];
        let remote_nodes = vec![FileNode::new_file("a.txt")];
        let mut state = AppState::new(
            make_test_tree(local_nodes.clone()),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_left_tree = Some(local_nodes);
        state.scan_right_tree = Some(vec![FileNode::new_file("a.txt")]);
        state.diff_filter_mode = true;
        state.rebuild_flat_nodes();
        let nodes_in_filter = state.flat_nodes.len();
        state.clear_scan_cache();
        assert!(!state.diff_filter_mode);
        assert!(state.flat_nodes.len() >= nodes_in_filter);
    }

    #[test]
    fn test_diff_filter_mode_hides_equal() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes.clone()),
            make_test_tree(remote_nodes.clone()),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.tree_cursor = 0;
        state.toggle_expand();
        state
            .left_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .right_cache
            .insert("src/a.ts".to_string(), "new".to_string());
        state
            .left_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        state
            .right_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        state.set_scan_result(local_nodes, remote_nodes);
        state.rebuild_flat_nodes();

        assert!(!state.diff_filter_mode);
        let all_files: Vec<&str> = state
            .flat_nodes
            .iter()
            .filter(|n| !n.is_dir)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(all_files.len(), 2);

        state.toggle_diff_filter();
        assert!(state.diff_filter_mode);
        let filtered_files: Vec<&str> = state
            .flat_nodes
            .iter()
            .filter(|n| !n.is_dir)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(filtered_files.len(), 1);
        assert_eq!(filtered_files[0], "src/a.ts");

        state.toggle_diff_filter();
        assert!(!state.diff_filter_mode);
        let all_again: Vec<&str> = state
            .flat_nodes
            .iter()
            .filter(|n| !n.is_dir)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(all_again.len(), 2);
    }

    #[test]
    fn test_diff_filter_hides_equal_dirs() {
        let local_nodes = vec![
            FileNode::new_dir_with_children("src", vec![FileNode::new_file("a.ts")]),
            FileNode::new_dir_with_children("test", vec![FileNode::new_file("t.ts")]),
        ];
        let remote_nodes = vec![
            FileNode::new_dir_with_children("src", vec![FileNode::new_file("a.ts")]),
            FileNode::new_dir_with_children("test", vec![FileNode::new_file("t.ts")]),
        ];
        let mut state = AppState::new(
            make_test_tree(local_nodes.clone()),
            make_test_tree(remote_nodes.clone()),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.tree_cursor = 0;
        state.toggle_expand();
        state.tree_cursor = 2;
        state.toggle_expand();
        state
            .left_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .right_cache
            .insert("src/a.ts".to_string(), "new".to_string());
        state
            .left_cache
            .insert("test/t.ts".to_string(), "same".to_string());
        state
            .right_cache
            .insert("test/t.ts".to_string(), "same".to_string());
        state.set_scan_result(local_nodes, remote_nodes);
        state.toggle_diff_filter();

        let names: Vec<&str> = state.flat_nodes.iter().map(|n| n.path.as_str()).collect();
        assert!(names.contains(&"src"));
        assert!(names.contains(&"src/a.ts"));
        assert!(!names.contains(&"test"));
        assert!(!names.contains(&"test/t.ts"));
    }
}
