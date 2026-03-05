//! ファイル選択・読み込み・キャッシュ管理。

use std::path::Path;

use crate::diff::engine::{self, DiffResult};

use super::AppState;

impl AppState {
    /// 現在カーソル位置のファイルを選択して diff を計算する
    pub fn select_file(&mut self) {
        let path = match self.flat_nodes.get(self.tree_cursor) {
            Some(n) if !n.is_dir => n.path.clone(),
            _ => return,
        };

        self.selected_path = Some(path.clone());

        // キャッシュからコンテンツを取得して diff
        let local_content = self.local_cache.get(&path).map(|s| s.as_str());
        let remote_content = self.remote_cache.get(&path).map(|s| s.as_str());

        // ツリー上の存在を確認（キャッシュ未ロードと存在しないを区別）
        let in_local_tree = self.local_tree.find_node(Path::new(&path)).is_some();
        let in_remote_tree = self.remote_tree.find_node(Path::new(&path)).is_some();

        self.current_diff = match (local_content, remote_content) {
            (Some(l), Some(r)) => {
                if engine::is_binary(l.as_bytes()) || engine::is_binary(r.as_bytes()) {
                    Some(DiffResult::Binary)
                } else {
                    Some(engine::compute_diff(l, r))
                }
            }
            (Some(l), None) => {
                if in_remote_tree {
                    // ツリー上にはリモートファイルあり→キャッシュ未ロード
                    self.status_message = format!("{}: remote content not loaded", path);
                } else {
                    self.status_message = format!("{}: local only", path);
                }
                // 片方だけでも diff 表示（空文字列との比較）
                if engine::is_binary(l.as_bytes()) {
                    Some(DiffResult::Binary)
                } else {
                    Some(engine::compute_diff(l, ""))
                }
            }
            (None, Some(r)) => {
                if in_local_tree {
                    self.status_message = format!("{}: local content not loaded", path);
                } else {
                    self.status_message = format!("{}: remote only", path);
                }
                if engine::is_binary(r.as_bytes()) {
                    Some(DiffResult::Binary)
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

    /// コンテンツキャッシュをクリアする (r キー)
    pub fn clear_cache(&mut self) {
        self.local_cache.clear();
        self.remote_cache.clear();
        self.error_paths.clear();
        self.current_diff = None;
        self.selected_path = None;
        self.status_message = "Cache cleared".to_string();
    }
}
