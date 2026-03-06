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
