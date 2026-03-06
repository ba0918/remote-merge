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
        // find_node は未ロードディレクトリの子を見つけられないため、
        // find_node_or_unloaded で「途中が未ロード」も区別する。
        // 「xx only」は確実に存在しないと言えるときだけ表示する。
        let remote_absent = self.remote_tree.find_node_or_unloaded(Path::new(&path))
            == crate::tree::NodePresence::NotFound;
        let local_absent = self.local_tree.find_node_or_unloaded(Path::new(&path))
            == crate::tree::NodePresence::NotFound;

        self.current_diff = match (local_content, remote_content) {
            (Some(l), Some(r)) => {
                if engine::is_binary(l.as_bytes()) || engine::is_binary(r.as_bytes()) {
                    Some(DiffResult::Binary)
                } else {
                    Some(engine::compute_diff(l, r))
                }
            }
            (Some(l), None) => {
                if remote_absent {
                    self.status_message = format!("{}: local only", path);
                } else {
                    self.status_message = format!("{}: remote content not loaded", path);
                }
                // 片方だけでも diff 表示（空文字列との比較）
                if engine::is_binary(l.as_bytes()) {
                    Some(DiffResult::Binary)
                } else {
                    Some(engine::compute_diff(l, ""))
                }
            }
            (None, Some(r)) => {
                if local_absent {
                    self.status_message = format!("{}: remote only", path);
                } else {
                    self.status_message = format!("{}: local content not loaded", path);
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

        // シンタックスハイライトキャッシュを構築
        if self.syntax_highlight_enabled {
            self.build_highlight_cache(&path);
        }
    }

    /// 指定パスのシンタックスハイライトキャッシュを構築する
    fn build_highlight_cache(&mut self, path: &str) {
        if let Some(content) = self.local_cache.get(path) {
            if self.highlight_cache_local.get(path).is_none() {
                let highlighted = self.highlighter.highlight_file(path, content);
                self.highlight_cache_local
                    .insert(path.to_string(), highlighted);
            }
        }
        if let Some(content) = self.remote_cache.get(path) {
            if self.highlight_cache_remote.get(path).is_none() {
                let highlighted = self.highlighter.highlight_file(path, content);
                self.highlight_cache_remote
                    .insert(path.to_string(), highlighted);
            }
        }
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
        self.highlight_cache_local.clear();
        self.highlight_cache_remote.clear();
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
        self.highlight_cache_local.clear();
        self.highlight_cache_remote.clear();
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
