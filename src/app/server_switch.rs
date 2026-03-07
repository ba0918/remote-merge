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
