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

            let (diff_files, unchecked_dirs) = self.collect_diff_files_under(&node.path);

            if diff_files.is_empty() {
                self.dialog = DialogState::Info(format!("No differences found in {}/", node.path));
                return;
            }

            let mut batch =
                BatchConfirmDialog::new(diff_files, direction, source, target, unchecked_dirs);
            batch.check_sensitive(&self.sensitive_patterns);
            self.dialog = DialogState::BatchConfirm(batch);
        } else {
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
        if self.selected_path.as_deref() == Some(path) {
            self.select_file();
        }
        self.rebuild_flat_nodes();
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
