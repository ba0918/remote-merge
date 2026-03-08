//! ダイアログ表示・操作。

use crate::merge::executor::MergeDirection;
use crate::ui::dialog::{
    BatchConfirmDialog, ConfirmDialog, DialogState, FilterPanel, HelpOverlay, PairServerMenu,
    ServerMenu,
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
                Badge::Modified | Badge::LeftOnly | Badge::RightOnly | Badge::Unchecked => {
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
            MergeDirection::LeftToRight => (
                self.left_source.display_name().to_string(),
                self.right_source.display_name().to_string(),
            ),
            MergeDirection::RightToLeft => (
                self.right_source.display_name().to_string(),
                self.left_source.display_name().to_string(),
            ),
        };

        if node.is_dir {
            // SSH 接続チェック: マージ先がリモートなら接続必須
            let target_side = match direction {
                MergeDirection::LeftToRight => &self.right_source,
                MergeDirection::RightToLeft => &self.left_source,
            };
            if target_side.is_remote() && !self.is_connected {
                self.status_message = "SSH not connected: cannot merge".to_string();
                return;
            }

            // ツリーから直接ファイル収集（expanded_dirs に依存しない）
            let all_files = super::merge_collect::collect_merge_files(
                &self.left_tree,
                &self.right_tree,
                &node.path,
            );

            // Badge を計算してフィルタリング（Unchecked は除外）
            // マージ方向に応じて不要な *-Only ファイルも除外する:
            //   LeftToRight: LeftOnly + Modified のみ（RightOnly は上書き対象がない）
            //   RightToLeft: RightOnly + Modified のみ（LeftOnly は上書き対象がない）
            let mut unchecked_count = 0usize;
            let diff_files: Vec<(String, Badge)> = all_files
                .into_iter()
                .map(|path| {
                    let badge = self.compute_badge(&path, false);
                    (path, badge)
                })
                .filter(|(_, badge)| {
                    if *badge == Badge::Unchecked {
                        unchecked_count += 1;
                        return false;
                    }
                    match direction {
                        MergeDirection::LeftToRight => {
                            matches!(badge, Badge::Modified | Badge::LeftOnly)
                        }
                        MergeDirection::RightToLeft => {
                            matches!(badge, Badge::Modified | Badge::RightOnly)
                        }
                    }
                })
                .collect();

            if diff_files.is_empty() {
                if unchecked_count > 0 {
                    self.dialog = DialogState::Info(format!(
                        "Cannot merge {}/: {} file(s) have unknown diff status. Open files to check diffs first",
                        node.path, unchecked_count
                    ));
                } else {
                    self.dialog =
                        DialogState::Info(format!("No differences found in {}/", node.path));
                }
                return;
            }

            let mut batch =
                BatchConfirmDialog::new(diff_files, direction, source, target, unchecked_count);
            batch.check_sensitive(&self.sensitive_patterns);
            self.dialog = DialogState::BatchConfirm(batch);
        } else {
            // 差分がなければ Info ダイアログ
            // Badge::Equal または current_diff の内容が同一ならブロック
            let badge = self.compute_badge(&node.path, false);
            let diff_equal = self.current_diff.as_ref().is_some_and(|d| d.is_equal());
            if badge == Badge::Equal || diff_equal {
                self.dialog = DialogState::Info(format!("No differences found in {}", node.path));
                return;
            }
            self.dialog = DialogState::Confirm(
                ConfirmDialog::new(node.path, direction, source, target)
                    .with_remote_to_remote(self.is_remote_to_remote()),
            );
        }
    }

    /// サーバ選択メニューを表示する (s キー)
    ///
    /// 左側がリモートの場合、同じサーバを右側に選択しても無意味なため除外する。
    pub fn show_server_menu(&mut self) {
        let left_server = self.left_source.server_name();
        let servers: Vec<String> = self
            .available_servers
            .iter()
            .filter(|s| left_server != Some(s.as_str()))
            .cloned()
            .collect();
        if servers.is_empty() {
            self.status_message = "No servers available".to_string();
            return;
        }
        self.dialog = DialogState::ServerSelect(ServerMenu::new(
            servers,
            self.right_source.display_name().to_string(),
        ));
    }

    /// ペアサーバ選択メニューを表示する（3way diff 時の s キー）
    ///
    /// 3つ以上のサーバが利用可能な場合、LEFT/RIGHT 両方を選択できるメニューを表示する。
    /// 2つ以下の場合は従来の show_server_menu にフォールバック。
    pub fn show_pair_server_menu(&mut self) {
        // "local" を先頭に、全サーバをリストアップ
        let mut servers = vec!["local".to_string()];
        for s in &self.available_servers {
            if !servers.contains(s) {
                servers.push(s.clone());
            }
        }

        if servers.len() < 2 {
            self.status_message = "No servers available".to_string();
            return;
        }

        // 2サーバしかない場合は従来メニュー
        if servers.len() == 2 {
            self.show_server_menu();
            return;
        }

        let left_name = self.left_source.display_name().to_string();
        let right_name = self.right_source.display_name().to_string();
        self.dialog =
            DialogState::PairServerSelect(PairServerMenu::new(servers, &left_name, &right_name));
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
            MergeDirection::LeftToRight => {
                self.right_cache
                    .insert(path.to_string(), content.to_string());
            }
            MergeDirection::RightToLeft => {
                self.left_cache
                    .insert(path.to_string(), content.to_string());
            }
        }
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

    fn make_flat_file(path: &str, badge: Badge) -> FlatNode {
        FlatNode {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            depth: 0,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge,
            ref_only: false,
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
            ref_only: false,
        }
    }

    #[test]
    fn test_collect_diff_files_under_modified() {
        let mut state = make_state();
        state.flat_nodes = vec![
            make_flat_dir("src", Badge::Unchecked, true),
            make_flat_file("src/a.rs", Badge::Modified),
            make_flat_file("src/b.rs", Badge::Equal),
            make_flat_file("src/c.rs", Badge::LeftOnly),
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
        state.show_merge_dialog(MergeDirection::LeftToRight);
        assert!(state.status_message.contains("Select a file"));
    }

    #[test]
    fn test_show_merge_dialog_unchecked_file_with_cache_equal() {
        // Unchecked でもキャッシュに同一内容があれば Equal として Info ダイアログ
        let mut state = make_state();
        let node = FileNode::new_file("a.rs");
        state.left_tree = make_test_tree(vec![node.clone()]);
        state.right_tree = make_test_tree(vec![node]);
        state.flat_nodes = vec![make_flat_file("a.rs", Badge::Unchecked)];
        // キャッシュに同一内容をセットすると compute_badge が Equal を返す
        state
            .left_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "same".to_string());
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        assert!(
            matches!(state.dialog, DialogState::Info(_)),
            "Expected Info dialog for equal content, got: {:?}",
            state.dialog
        );
    }

    #[test]
    fn test_show_merge_dialog_unchecked_file_with_cache_diff() {
        // Unchecked でもキャッシュに異なる内容があれば Confirm ダイアログ
        let mut state = make_state();
        let node = FileNode::new_file("a.rs");
        state.left_tree = make_test_tree(vec![node.clone()]);
        state.right_tree = make_test_tree(vec![node]);
        state.flat_nodes = vec![make_flat_file("a.rs", Badge::Unchecked)];
        state
            .left_cache
            .insert("a.rs".to_string(), "old".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "new".to_string());
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        assert!(
            matches!(state.dialog, DialogState::Confirm(_)),
            "Expected Confirm dialog for different content, got: {:?}",
            state.dialog
        );
    }

    #[test]
    fn test_show_merge_dialog_equal_file() {
        let mut state = make_state();
        let node = FileNode::new_file("a.rs");
        state.left_tree = make_test_tree(vec![node.clone()]);
        state.right_tree = make_test_tree(vec![node]);
        state.flat_nodes = vec![make_flat_file("a.rs", Badge::Equal)];
        state
            .left_cache
            .insert("a.rs".to_string(), "same".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "same".to_string());
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        assert!(matches!(state.dialog, DialogState::Info(_)));
    }

    #[test]
    fn test_show_merge_dialog_modified_file() {
        let mut state = make_state();
        let node = FileNode::new_file("a.rs");
        state.left_tree = make_test_tree(vec![node.clone()]);
        state.right_tree = make_test_tree(vec![node]);
        state.flat_nodes = vec![make_flat_file("a.rs", Badge::Modified)];
        state
            .left_cache
            .insert("a.rs".to_string(), "old".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "new".to_string());
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
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
    fn test_show_server_menu_excludes_left_remote_server() {
        // left_source がリモートの場合、同じサーバはリストから除外される
        let mut state = make_state();
        state.left_source = Side::Remote("develop".to_string());
        state.right_source = Side::Remote("staging".to_string());
        state.available_servers = vec![
            "develop".to_string(),
            "staging".to_string(),
            "release".to_string(),
        ];
        state.show_server_menu();
        match &state.dialog {
            DialogState::ServerSelect(menu) => {
                assert_eq!(menu.servers.len(), 2);
                assert!(!menu.servers.contains(&"develop".to_string()));
                assert!(menu.servers.contains(&"staging".to_string()));
                assert!(menu.servers.contains(&"release".to_string()));
            }
            other => panic!("Expected ServerSelect dialog, got: {:?}", other),
        }
    }

    #[test]
    fn test_show_server_menu_local_left_keeps_all() {
        // left_source が Local の場合、全サーバが表示される
        let mut state = make_state();
        state.left_source = Side::Local;
        state.available_servers = vec!["develop".to_string(), "staging".to_string()];
        state.show_server_menu();
        match &state.dialog {
            DialogState::ServerSelect(menu) => {
                assert_eq!(menu.servers.len(), 2);
            }
            other => panic!("Expected ServerSelect dialog, got: {:?}", other),
        }
    }

    #[test]
    fn test_show_server_menu_only_left_server_available() {
        // left_source と同じサーバしか available にない → 空 → "No servers available"
        let mut state = make_state();
        state.left_source = Side::Remote("develop".to_string());
        state.available_servers = vec!["develop".to_string()];
        state.show_server_menu();
        assert!(state.status_message.contains("No servers"));
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
        state.sync_cache_after_merge("a.rs", "content", MergeDirection::LeftToRight);
        assert_eq!(state.right_cache.get("a.rs").unwrap(), "content");
    }

    #[test]
    fn test_update_badge_after_merge() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state
            .left_cache
            .insert("test.txt".to_string(), "content".to_string());
        state.update_badge_after_merge("test.txt", "content", MergeDirection::LeftToRight);
        assert_eq!(state.right_cache.get("test.txt").unwrap(), "content");
    }

    #[test]
    fn test_sync_cache_after_merge_remote_to_local() {
        let mut state = make_state();
        state.sync_cache_after_merge("a.rs", "content", MergeDirection::RightToLeft);
        assert_eq!(state.left_cache.get("a.rs").unwrap(), "content");
    }

    #[test]
    fn test_switch_server() {
        let mut state = make_state();
        state.left_cache.insert("a.rs".to_string(), "x".to_string());
        state
            .right_cache
            .insert("a.rs".to_string(), "y".to_string());
        state.selected_path = Some("a.rs".to_string());
        state.current_diff = Some(crate::diff::engine::DiffResult::Equal);

        let new_tree = make_test_tree(vec![FileNode::new_file("b.rs")]);
        state.switch_server(Side::new("staging"), new_tree);

        assert_eq!(state.right_source.display_name(), "staging");
        assert!(state.left_cache.is_empty());
        assert!(state.right_cache.is_empty());
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
    fn test_show_merge_dialog_dir_all_unchecked_blocks_merge() {
        // ディレクトリ配下が全て Unchecked の場合はマージ不可
        let mut state = make_state();
        state.is_connected = true;
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )];
        state.left_tree = make_test_tree(local_nodes);
        state.right_tree = make_test_tree(remote_nodes);
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        match &state.dialog {
            DialogState::Info(msg) => {
                assert!(
                    msg.contains("unknown diff status"),
                    "Expected unknown diff status message, got: {}",
                    msg
                );
            }
            other => panic!("Expected Info dialog, got: {:?}", other),
        }
    }

    #[test]
    fn test_show_merge_dialog_dir_mixed_unchecked_and_modified() {
        // Unchecked + Modified の場合、Modified のみがバッチマージに含まれる
        let mut state = make_state();
        state.is_connected = true;
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs"), FileNode::new_file("b.rs")],
        )];
        state.left_tree = make_test_tree(local_nodes);
        state.right_tree = make_test_tree(remote_nodes);
        // a.rs はキャッシュあり（Modified）、b.rs は未キャッシュ（Unchecked）
        state
            .left_cache
            .insert("src/a.rs".to_string(), "old".to_string());
        state
            .right_cache
            .insert("src/a.rs".to_string(), "new".to_string());
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        match &state.dialog {
            DialogState::BatchConfirm(batch) => {
                assert_eq!(batch.files.len(), 1);
                assert_eq!(batch.files[0].0, "src/a.rs");
                assert_eq!(batch.files[0].1, Badge::Modified);
                assert_eq!(batch.unchecked_count, 1); // b.rs が Unchecked
            }
            other => panic!("Expected BatchConfirm dialog, got: {:?}", other),
        }
    }

    #[test]
    fn test_show_merge_dialog_dir_no_connection() {
        let mut state = make_state();
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.is_connected = false;
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        assert!(state.status_message.contains("SSH not connected"));
    }

    #[test]
    fn test_show_merge_dialog_binary_equal_blocks_merge() {
        use crate::diff::binary::BinaryInfo;
        use crate::diff::engine::DiffResult;

        let mut state = make_state();
        state.flat_nodes = vec![make_flat_file("logo.png", Badge::Unchecked)];
        state.tree_cursor = 0;
        // SHA-256が同一のバイナリ
        let info = BinaryInfo {
            size: 100,
            sha256: "abc123".to_string(),
        };
        state.current_diff = Some(DiffResult::Binary {
            left: Some(info.clone()),
            right: Some(info),
        });
        state.show_merge_dialog(MergeDirection::LeftToRight);
        assert!(matches!(state.dialog, DialogState::Info(_)));
    }

    #[test]
    fn test_show_merge_dialog_symlink_equal_blocks_merge() {
        use crate::diff::engine::DiffResult;

        let mut state = make_state();
        state.flat_nodes = vec![make_flat_file("link", Badge::Unchecked)];
        state.tree_cursor = 0;
        state.current_diff = Some(DiffResult::SymlinkDiff {
            left_target: Some("../README.md".to_string()),
            right_target: Some("../README.md".to_string()),
        });
        state.show_merge_dialog(MergeDirection::LeftToRight);
        assert!(matches!(state.dialog, DialogState::Info(_)));
    }

    #[test]
    fn test_show_merge_dialog_dir_right_only_excluded_for_left_to_right() {
        // LeftToRight: RightOnly ファイルはマージ対象外（上書き元がないため）
        let mut state = make_state();
        state.is_connected = true;
        let local_nodes = vec![FileNode::new_dir_with_children("src", vec![])];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("staging_only.rs")],
        )];
        state.left_tree = make_test_tree(local_nodes);
        state.right_tree = make_test_tree(remote_nodes);
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        // RightOnly のみなので差分なし → Info ダイアログ
        assert!(
            matches!(state.dialog, DialogState::Info(_)),
            "Expected Info dialog (no mergeable files), got: {:?}",
            state.dialog
        );
    }

    #[test]
    fn test_show_merge_dialog_dir_left_only_excluded_for_right_to_left() {
        // RightToLeft: LeftOnly ファイルはマージ対象外
        let mut state = make_state();
        state.is_connected = true;
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("local_only.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children("src", vec![])];
        state.left_tree = make_test_tree(local_nodes);
        state.right_tree = make_test_tree(remote_nodes);
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::RightToLeft);
        assert!(
            matches!(state.dialog, DialogState::Info(_)),
            "Expected Info dialog (no mergeable files), got: {:?}",
            state.dialog
        );
    }

    #[test]
    fn test_show_merge_dialog_dir_right_local_left_to_right_no_connection_ok() {
        // right_source = Local + LeftToRight + is_connected=false でもマージ可能
        // （target=Local なので SSH 不要）
        let mut state = make_state();
        state.left_source = Side::Remote("develop".to_string());
        state.right_source = Side::Local;
        state.is_connected = false;
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.rs")],
        )];
        state.left_tree = make_test_tree(local_nodes);
        state.right_tree = make_test_tree(remote_nodes);
        state
            .left_cache
            .insert("src/a.rs".to_string(), "old".to_string());
        state
            .right_cache
            .insert("src/a.rs".to_string(), "new".to_string());
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftToRight);
        // target=Local なので接続不要 → BatchConfirm が表示される
        assert!(
            matches!(state.dialog, DialogState::BatchConfirm(_)),
            "Expected BatchConfirm dialog (target=Local, no SSH needed), got: {:?}",
            state.dialog
        );
    }

    #[test]
    fn test_show_merge_dialog_dir_left_remote_right_to_left_no_connection_blocked() {
        // left_source = Remote + RightToLeft + is_connected=false でブロック
        // （target=Remote で SSH 必要）
        let mut state = make_state();
        state.left_source = Side::Remote("develop".to_string());
        state.right_source = Side::Local;
        state.is_connected = false;
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::RightToLeft);
        assert!(
            state.status_message.contains("SSH not connected"),
            "Expected SSH not connected message, got: {}",
            state.status_message
        );
    }

    #[test]
    fn test_show_merge_dialog_dir_right_only_included_for_right_to_left() {
        // RightToLeft: RightOnly ファイルはマージ対象に含まれる
        let mut state = make_state();
        state.is_connected = true;
        let local_nodes = vec![FileNode::new_dir_with_children("src", vec![])];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("remote_only.rs")],
        )];
        state.left_tree = make_test_tree(local_nodes);
        state.right_tree = make_test_tree(remote_nodes);
        // キャッシュがないと Unchecked 扱いになるので、右キャッシュだけ入れる
        state
            .right_cache
            .insert("src/remote_only.rs".to_string(), "content".to_string());
        state.flat_nodes = vec![make_flat_dir("src", Badge::Unchecked, true)];
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::RightToLeft);
        match &state.dialog {
            DialogState::BatchConfirm(batch) => {
                assert_eq!(batch.files.len(), 1);
                assert_eq!(batch.files[0].0, "src/remote_only.rs");
                assert_eq!(batch.files[0].1, Badge::RightOnly);
            }
            other => panic!("Expected BatchConfirm dialog, got: {:?}", other),
        }
    }
}
