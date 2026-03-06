//! ダイアログ表示・操作。

use crate::merge::executor::MergeDirection;
use crate::tree::FileTree;
use crate::ui::dialog::{
    BatchConfirmDialog, ConfirmDialog, DialogState, FilterPanel, HelpOverlay, ServerMenu,
};

use super::types::Badge;
use super::AppState;

impl AppState {
    /// ディレクトリ配下の差分ファイルを収集する
    ///
    /// 展開済みノードのみを対象とし、未展開ディレクトリの数も返す。
    pub fn collect_diff_files_under(&self, dir_path: &str) -> (Vec<(String, Badge)>, usize) {
        let prefix = format!("{}/", dir_path);
        let mut diff_files = Vec::new();
        let mut unchecked_dirs = 0;

        for node in &self.flat_nodes {
            if !node.path.starts_with(&prefix) {
                continue;
            }

            if node.is_dir {
                if !node.expanded && node.badge == Badge::Unchecked {
                    unchecked_dirs += 1;
                }
                continue;
            }

            match node.badge {
                Badge::Modified | Badge::LocalOnly | Badge::RemoteOnly | Badge::Unchecked => {
                    diff_files.push((node.path.clone(), node.badge));
                }
                _ => {}
            }
        }

        (diff_files, unchecked_dirs)
    }

    /// マージ確認ダイアログを表示する (Shift+L / Shift+R)
    pub fn show_merge_dialog(&mut self, direction: MergeDirection) {
        let node = match self.flat_nodes.get(self.tree_cursor) {
            Some(n) => n.clone(),
            None => {
                self.status_message = "Select a file first".to_string();
                return;
            }
        };

        let (source, target) = match direction {
            MergeDirection::LocalToRemote => ("local".to_string(), self.server_name.clone()),
            MergeDirection::RemoteToLocal => (self.server_name.clone(), "local".to_string()),
        };

        if node.is_dir {
            if !self.is_connected && matches!(direction, MergeDirection::LocalToRemote) {
                self.status_message = "SSH not connected: cannot merge".to_string();
                return;
            }

            // ツリーから直接ファイル収集（expanded_dirs に依存しない）
            let all_files = super::merge_collect::collect_merge_files(
                &self.local_tree,
                &self.remote_tree,
                &node.path,
            );

            // Badge を計算してフィルタリング
            let diff_files: Vec<(String, Badge)> = all_files
                .into_iter()
                .map(|path| {
                    let badge = self.compute_badge(&path, false);
                    (path, badge)
                })
                .filter(|(_, badge)| {
                    matches!(
                        badge,
                        Badge::Modified | Badge::LocalOnly | Badge::RemoteOnly | Badge::Unchecked
                    )
                })
                .collect();

            if diff_files.is_empty() {
                self.dialog = DialogState::Info(format!("No differences found in {}/", node.path));
                return;
            }

            let mut batch = BatchConfirmDialog::new(diff_files, direction, source, target, 0);
            batch.check_sensitive(&self.sensitive_patterns);
            self.dialog = DialogState::BatchConfirm(batch);
        } else {
            // 差分がなければ Info ダイアログ
            let badge = self.compute_badge(&node.path, false);
            if badge == Badge::Equal {
                self.dialog = DialogState::Info(format!("No differences found in {}", node.path));
                return;
            }
            self.dialog =
                DialogState::Confirm(ConfirmDialog::new(node.path, direction, source, target));
        }
    }

    /// サーバ選択メニューを表示する (s キー)
    pub fn show_server_menu(&mut self) {
        if self.available_servers.is_empty() {
            self.status_message = "No servers available".to_string();
            return;
        }
        self.dialog = DialogState::ServerSelect(ServerMenu::new(
            self.available_servers.clone(),
            self.server_name.clone(),
        ));
    }

    /// ヘルプオーバーレイを表示する (? キー)
    pub fn show_help(&mut self) {
        self.dialog = DialogState::Help(HelpOverlay::new());
    }

    /// ダイアログを閉じる
    pub fn close_dialog(&mut self) {
        self.dialog = DialogState::None;
    }

    /// ダイアログが表示中かどうか
    pub fn has_dialog(&self) -> bool {
        !matches!(self.dialog, DialogState::None)
    }

    /// マージ完了後にバッジを更新する
    pub fn update_badge_after_merge(
        &mut self,
        path: &str,
        content: &str,
        direction: MergeDirection,
    ) {
        self.sync_cache_after_merge(path, content, direction);
        if self.selected_path.as_deref() == Some(path) {
            self.select_file();
        }
        self.rebuild_flat_nodes();
    }

    /// マージ後のキャッシュ同期のみ（rebuild_flat_nodes は呼ばない）
    ///
    /// バッチマージ時は最後に1回だけ rebuild_flat_nodes を呼ぶため、
    /// 個別ファイルごとにはキャッシュ同期のみ行う。
    pub fn sync_cache_after_merge(&mut self, path: &str, content: &str, direction: MergeDirection) {
        match direction {
            MergeDirection::LocalToRemote => {
                self.remote_cache
                    .insert(path.to_string(), content.to_string());
            }
            MergeDirection::RemoteToLocal => {
                self.local_cache
                    .insert(path.to_string(), content.to_string());
            }
        }
    }

    /// サーバ切替後にツリーを再構築する
    pub fn switch_server(&mut self, new_server: String, remote_tree: FileTree) {
        self.status_message = format!("local <-> {} | Tab: switch focus | q: quit", &new_server);
        self.server_name = new_server;
        self.remote_tree = remote_tree;
        self.local_cache.clear();
        self.remote_cache.clear();
        self.error_paths.clear();
        self.current_diff = None;
        self.selected_path = None;
        self.diff_scroll = 0;
        self.diff_cursor = 0;
        self.undo_stack.clear();
        self.clear_scan_cache();
        self.rebuild_flat_nodes();
        self.is_connected = true;
    }

    /// フィルターパネルを表示する (f キー)
    pub fn show_filter_panel(&mut self) {
        if self.exclude_patterns.is_empty() {
            self.status_message = "No exclude patterns configured".to_string();
            return;
        }
        let mut panel = FilterPanel::new(&self.exclude_patterns);
        for (pattern, enabled) in &mut panel.patterns {
            if self.disabled_patterns.contains(pattern) {
                *enabled = false;
            }
        }
        self.dialog = DialogState::Filter(panel);
    }

    /// フィルターパネルの変更を適用する
    pub fn apply_filter_changes(&mut self, panel: &FilterPanel) {
        self.disabled_patterns.clear();
        for (pattern, enabled) in &panel.patterns {
            if !enabled {
                self.disabled_patterns.insert(pattern.clone());
            }
        }
        self.rebuild_flat_nodes();
    }

    /// 現在有効な除外パターンを返す
    pub fn active_exclude_patterns(&self) -> Vec<String> {
        self.exclude_patterns
            .iter()
            .filter(|p| !self.disabled_patterns.contains(*p))
            .cloned()
            .collect()
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

    fn make_state() -> AppState {
        AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        )
    }

    fn make_flat_file(path: &str, badge: Badge) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge,
        }
    }

    fn make_flat_dir(path: &str, badge: Badge, expanded: bool) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: 0,
            is_dir: true,
            is_symlink: false,
            expanded,
            badge,
        }
    }

    #[test]
    fn test_collect_diff_files_under_modified() {
        let mut state = make_state();
        state.flat_nodes = vec![
            make_flat_dir("src", Badge::Unchecked, true),
            make_flat_file("src/a.rs", Badge::Modified),
            make_flat_file("src/b.rs", Badge::Equal),
            make_flat_file("src/c.rs", Badge::LocalOnly),
        ];
        let (files, unchecked) = state.collect_diff_files_under("src");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "src/a.rs");
        assert_eq!(files[1].0, "src/c.rs");
        assert_eq!(unchecked, 0);
    }

    #[test]
    fn test_collect_diff_files_under_unchecked_dirs() {
        let mut state = make_state();
        state.flat_nodes = vec![
            make_flat_dir("src", Badge::Unchecked, true),
            make_flat_dir("src/sub", Badge::Unchecked, false),
        ];
        let (files, unchecked) = state.collect_diff_files_under("src");
        assert_eq!(files.len(), 0);
        assert_eq!(unchecked, 1);
    }

    #[test]
    fn test_collect_diff_files_under_no_prefix_match() {
        let mut state = make_state();
        state.flat_nodes = vec![make_flat_file("other/a.rs", Badge::Modified)];
        let (files, unchecked) = state.collect_diff_files_under("src");
        assert_eq!(files.len(), 0);
        assert_eq!(unchecked, 0);
    }

    #[test]
    fn test_show_merge_dialog_no_cursor() {
        let mut state = make_state();
        state.flat_nodes.clear();
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(state.status_message.contains("Select a file"));
    }

    #[test]
    fn test_show_merge_dialog_equal_file() {
        let mut state = make_state();
        let node = FileNode::new_file("a.rs");
        state.local_tree = make_test_tree(vec![node.clone()]);
        state.remote_tree = make_test_tree(vec![node]);
        state.flat_nodes = vec![make_flat_file("a.rs", Badge::Equal)];
        state
            .local_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "same".to_string());
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(matches!(state.dialog, DialogState::Info(_)));
    }

    #[test]
    fn test_show_merge_dialog_modified_file() {
        let mut state = make_state();
        let node = FileNode::new_file("a.rs");
        state.local_tree = make_test_tree(vec![node.clone()]);
        state.remote_tree = make_test_tree(vec![node]);
        state.flat_nodes = vec![make_flat_file("a.rs", Badge::Modified)];
        state
            .local_cache
            .insert("a.rs".to_string(), "old".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "new".to_string());
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(matches!(state.dialog, DialogState::Confirm(_)));
    }

    #[test]
    fn test_show_server_menu_empty() {
        let mut state = make_state();
        state.available_servers.clear();
        state.show_server_menu();
        assert!(state.status_message.contains("No servers"));
    }

    #[test]
    fn test_show_server_menu_with_servers() {
        let mut state = make_state();
        state.available_servers = vec!["staging".to_string()];
        state.show_server_menu();
        assert!(matches!(state.dialog, DialogState::ServerSelect(_)));
    }

    #[test]
    fn test_show_help() {
        let mut state = make_state();
        state.show_help();
        assert!(matches!(state.dialog, DialogState::Help(_)));
    }

    #[test]
    fn test_close_dialog() {
        let mut state = make_state();
        state.show_help();
        assert!(state.has_dialog());
        state.close_dialog();
        assert!(!state.has_dialog());
    }

    #[test]
    fn test_sync_cache_after_merge_local_to_remote() {
        let mut state = make_state();
        state.sync_cache_after_merge("a.rs", "content", MergeDirection::LocalToRemote);
        assert_eq!(state.remote_cache.get("a.rs").unwrap(), "content");
    }

    #[test]
    fn test_sync_cache_after_merge_remote_to_local() {
        let mut state = make_state();
        state.sync_cache_after_merge("a.rs", "content", MergeDirection::RemoteToLocal);
        assert_eq!(state.local_cache.get("a.rs").unwrap(), "content");
    }

    #[test]
    fn test_switch_server() {
        let mut state = make_state();
        state
            .local_cache
            .insert("a.rs".to_string(), "x".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "y".to_string());
        state.selected_path = Some("a.rs".to_string());
        state.current_diff = Some(crate::diff::engine::DiffResult::Equal);

        let new_tree = make_test_tree(vec![FileNode::new_file("b.rs")]);
        state.switch_server("staging".to_string(), new_tree);

        assert_eq!(state.server_name, "staging");
        assert!(state.local_cache.is_empty());
        assert!(state.remote_cache.is_empty());
        assert!(state.selected_path.is_none());
        assert!(state.current_diff.is_none());
        assert!(state.is_connected);
        assert_eq!(state.diff_scroll, 0);
        assert_eq!(state.diff_cursor, 0);
    }

    #[test]
    fn test_show_filter_panel_empty() {
        let mut state = make_state();
        state.exclude_patterns.clear();
        state.show_filter_panel();
        assert!(state.status_message.contains("No exclude patterns"));
    }

    #[test]
    fn test_show_filter_panel_with_patterns() {
        let mut state = make_state();
        state.exclude_patterns = vec!["*.log".to_string(), "node_modules".to_string()];
        state.show_filter_panel();
        assert!(matches!(state.dialog, DialogState::Filter(_)));
    }

    #[test]
    fn test_show_filter_panel_respects_disabled() {
        let mut state = make_state();
        state.exclude_patterns = vec!["*.log".to_string(), "*.tmp".to_string()];
        state.disabled_patterns.insert("*.log".to_string());
        state.show_filter_panel();
        if let DialogState::Filter(panel) = &state.dialog {
            let log_entry = panel.patterns.iter().find(|(p, _)| p == "*.log");
            assert!(!log_entry.unwrap().1);
            let tmp_entry = panel.patterns.iter().find(|(p, _)| p == "*.tmp");
            assert!(tmp_entry.unwrap().1);
        } else {
            panic!("Expected Filter dialog");
        }
    }

    #[test]
    fn test_apply_filter_changes() {
        let mut state = make_state();
        state.exclude_patterns = vec!["*.log".to_string(), "*.tmp".to_string()];
        let panel = FilterPanel {
            patterns: vec![("*.log".to_string(), false), ("*.tmp".to_string(), true)],
            cursor: 0,
        };
        state.apply_filter_changes(&panel);
        assert!(state.disabled_patterns.contains("*.log"));
        assert!(!state.disabled_patterns.contains("*.tmp"));
    }

    #[test]
    fn test_active_exclude_patterns() {
        let mut state = make_state();
        state.exclude_patterns = vec!["*.log".to_string(), "*.tmp".to_string(), "dist".to_string()];
        state.disabled_patterns.insert("*.tmp".to_string());
        let active = state.active_exclude_patterns();
        assert_eq!(active, vec!["*.log".to_string(), "dist".to_string()]);
    }

    #[test]
    fn test_show_merge_dialog_dir_no_connection() {
        let mut state = make_state();
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.is_connected = false;
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(state.status_message.contains("SSH not connected"));
    }
}
