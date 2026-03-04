//! TUI アプリケーション状態管理。
//! ツリー、diff、フォーカス、コンテンツキャッシュを一元管理する。

use std::collections::HashMap;
use std::path::Path;

use crate::diff::engine::{self, DiffResult};
use crate::tree::{FileNode, FileTree};

/// TUI のフォーカス対象
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    FileTree,
    DiffView,
}

/// 差分バッジ（ファイル状態を示すマーカー）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// `[M]` Modified - 差分あり
    Modified,
    /// `[=]` Equal - 同一
    Equal,
    /// `[+]` Local Only
    LocalOnly,
    /// `[-]` Remote Only
    RemoteOnly,
    /// `[?]` Unchecked - 未比較
    Unchecked,
}

impl Badge {
    /// バッジの表示文字列
    pub fn label(&self) -> &'static str {
        match self {
            Badge::Modified => "[M]",
            Badge::Equal => "[=]",
            Badge::LocalOnly => "[+]",
            Badge::RemoteOnly => "[-]",
            Badge::Unchecked => "[?]",
        }
    }
}

/// フラット化されたツリーの1行を表す
#[derive(Debug, Clone)]
pub struct FlatNode {
    /// 表示パス（相対）
    pub path: String,
    /// ノード名
    pub name: String,
    /// インデント深さ
    pub depth: usize,
    /// ディレクトリか
    pub is_dir: bool,
    /// シンボリックリンクか
    pub is_symlink: bool,
    /// ディレクトリが展開されているか
    pub expanded: bool,
    /// 差分バッジ
    pub badge: Badge,
}

/// TUI アプリケーション全体の状態
pub struct AppState {
    /// 現在のフォーカス
    pub focus: Focus,
    /// ローカルファイルツリー
    pub local_tree: FileTree,
    /// リモートファイルツリー
    pub remote_tree: FileTree,
    /// 接続中のサーバ名
    pub server_name: String,
    /// ローカルファイル内容キャッシュ (パス → 内容)
    pub local_cache: HashMap<String, String>,
    /// リモートファイル内容キャッシュ (パス → 内容)
    pub remote_cache: HashMap<String, String>,
    /// 現在選択中の diff 結果
    pub current_diff: Option<DiffResult>,
    /// 現在選択中のファイルパス
    pub selected_path: Option<String>,
    /// フラット化されたツリー行リスト
    pub flat_nodes: Vec<FlatNode>,
    /// ツリーのカーソル位置
    pub tree_cursor: usize,
    /// diff ビューのスクロールオフセット
    pub diff_scroll: usize,
    /// 展開中ディレクトリの集合
    pub expanded_dirs: std::collections::HashSet<String>,
    /// アプリを終了するか
    pub should_quit: bool,
    /// ステータスバーに表示するメッセージ
    pub status_message: String,
}

impl AppState {
    /// 新しい AppState を構築する
    pub fn new(
        local_tree: FileTree,
        remote_tree: FileTree,
        server_name: String,
    ) -> Self {
        let mut state = Self {
            focus: Focus::FileTree,
            local_tree,
            remote_tree,
            server_name: server_name.clone(),
            local_cache: HashMap::new(),
            remote_cache: HashMap::new(),
            current_diff: None,
            selected_path: None,
            flat_nodes: Vec::new(),
            tree_cursor: 0,
            diff_scroll: 0,
            expanded_dirs: std::collections::HashSet::new(),
            should_quit: false,
            status_message: format!("local ↔ {} | Tab: switch focus | q: quit", server_name),
        };
        state.rebuild_flat_nodes();
        state
    }

    /// フォーカスを切り替える (Tab)
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::FileTree => Focus::DiffView,
            Focus::DiffView => Focus::FileTree,
        };
    }

    /// ツリーカーソルを上に移動
    pub fn cursor_up(&mut self) {
        if self.tree_cursor > 0 {
            self.tree_cursor -= 1;
        }
    }

    /// ツリーカーソルを下に移動
    pub fn cursor_down(&mut self) {
        if self.tree_cursor + 1 < self.flat_nodes.len() {
            self.tree_cursor += 1;
        }
    }

    /// diff ビューを上にスクロール
    pub fn scroll_up(&mut self) {
        self.diff_scroll = self.diff_scroll.saturating_sub(1);
    }

    /// diff ビューを下にスクロール
    pub fn scroll_down(&mut self) {
        self.diff_scroll = self.diff_scroll.saturating_add(1);
    }

    /// ディレクトリの展開/折りたたみを切り替える
    pub fn toggle_expand(&mut self) {
        if let Some(node) = self.flat_nodes.get(self.tree_cursor) {
            if node.is_dir {
                let path = node.path.clone();
                if self.expanded_dirs.contains(&path) {
                    self.expanded_dirs.remove(&path);
                } else {
                    self.expanded_dirs.insert(path);
                }
                self.rebuild_flat_nodes();
            }
        }
    }

    /// 現在カーソル位置のファイルを選択して diff を計算する
    pub fn select_file(&mut self) {
        let node = match self.flat_nodes.get(self.tree_cursor) {
            Some(n) if !n.is_dir => n.clone(),
            _ => return,
        };

        let path = node.path.clone();
        self.selected_path = Some(path.clone());

        // キャッシュからコンテンツを取得して diff
        let local_content = self.local_cache.get(&path).map(|s| s.as_str());
        let remote_content = self.remote_cache.get(&path).map(|s| s.as_str());

        self.current_diff = match (local_content, remote_content) {
            (Some(l), Some(r)) => {
                // バイナリチェック
                if engine::is_binary(l.as_bytes()) || engine::is_binary(r.as_bytes()) {
                    Some(DiffResult::Binary)
                } else {
                    Some(engine::compute_diff(l, r))
                }
            }
            (Some(_), None) => {
                self.status_message = format!("{}: local only", path);
                None
            }
            (None, Some(_)) => {
                self.status_message = format!("{}: remote only", path);
                None
            }
            (None, None) => {
                self.status_message = format!("{}: content not loaded", path);
                None
            }
        };
        self.diff_scroll = 0;
    }

    /// コンテンツキャッシュをクリアする (r キー)
    pub fn clear_cache(&mut self) {
        self.local_cache.clear();
        self.remote_cache.clear();
        self.current_diff = None;
        self.selected_path = None;
        self.status_message = "Cache cleared".to_string();
    }

    /// ローカル/リモートのツリーを比較してバッジを決定する
    pub fn compute_badge(&self, path: &str, is_dir: bool) -> Badge {
        if is_dir {
            return Badge::Unchecked;
        }
        let in_local = self.local_tree.find_node(Path::new(path)).is_some();
        let in_remote = self.remote_tree.find_node(Path::new(path)).is_some();

        match (in_local, in_remote) {
            (true, false) => Badge::LocalOnly,
            (false, true) => Badge::RemoteOnly,
            (true, true) => {
                // キャッシュに両方あれば diff で判定
                match (self.local_cache.get(path), self.remote_cache.get(path)) {
                    (Some(l), Some(r)) => {
                        if l == r {
                            Badge::Equal
                        } else {
                            Badge::Modified
                        }
                    }
                    _ => Badge::Unchecked,
                }
            }
            (false, false) => Badge::Unchecked,
        }
    }

    /// ファイルツリーをフラット化して flat_nodes を再構築する
    pub fn rebuild_flat_nodes(&mut self) {
        let mut nodes = Vec::new();
        // ローカルとリモートをマージしたツリーを構築
        let merged = self.merge_tree_nodes();
        for node in &merged {
            self.flatten_node(node, "", 0, &mut nodes);
        }
        self.flat_nodes = nodes;
        // カーソル位置を範囲内に収める
        if self.tree_cursor >= self.flat_nodes.len() && !self.flat_nodes.is_empty() {
            self.tree_cursor = self.flat_nodes.len() - 1;
        }
    }

    /// ローカルとリモートのルートノードをマージ（和集合）する
    fn merge_tree_nodes(&self) -> Vec<MergedNode> {
        merge_node_lists(&self.local_tree.nodes, &self.remote_tree.nodes)
    }

    /// 再帰的にフラット化する
    fn flatten_node(
        &self,
        node: &MergedNode,
        parent_path: &str,
        depth: usize,
        out: &mut Vec<FlatNode>,
    ) {
        let path = if parent_path.is_empty() {
            node.name.clone()
        } else {
            format!("{}/{}", parent_path, node.name)
        };

        let expanded = self.expanded_dirs.contains(&path);
        let badge = self.compute_badge(&path, node.is_dir);

        out.push(FlatNode {
            path: path.clone(),
            name: node.name.clone(),
            depth,
            is_dir: node.is_dir,
            is_symlink: node.is_symlink,
            expanded,
            badge,
        });

        if node.is_dir && expanded {
            for child in &node.children {
                self.flatten_node(child, &path, depth + 1, out);
            }
        }
    }
}

/// ツリーマージ用の一時ノード
#[derive(Debug, Clone)]
struct MergedNode {
    name: String,
    is_dir: bool,
    is_symlink: bool,
    children: Vec<MergedNode>,
}

/// 2つの FileNode リストをマージして MergedNode リストを返す
fn merge_node_lists(local: &[FileNode], remote: &[FileNode]) -> Vec<MergedNode> {
    let mut map: std::collections::BTreeMap<String, MergedNode> = std::collections::BTreeMap::new();

    for node in local {
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
        });
        if node.is_dir() {
            entry.is_dir = true;
            if let Some(children) = &node.children {
                entry.children = merge_node_lists(children, &entry.children_as_file_nodes_placeholder());
            }
        }
    }

    for node in remote {
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
        });
        if node.is_dir() {
            entry.is_dir = true;
            if let Some(children) = &node.children {
                // リモートの子ノードもマージ
                let existing = std::mem::take(&mut entry.children);
                entry.children = merge_merged_with_file_nodes(&existing, children);
            }
        }
    }

    // ディレクトリ優先、名前順でソート
    let mut result: Vec<MergedNode> = map.into_values().collect();
    result.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });
    result
}

impl MergedNode {
    fn children_as_file_nodes_placeholder(&self) -> Vec<FileNode> {
        // MergedNode の children から比較用の空リストを返す
        Vec::new()
    }
}

/// MergedNode リストと FileNode リストをマージ
fn merge_merged_with_file_nodes(merged: &[MergedNode], file_nodes: &[FileNode]) -> Vec<MergedNode> {
    let mut map: std::collections::BTreeMap<String, MergedNode> = std::collections::BTreeMap::new();

    for m in merged {
        map.insert(m.name.clone(), m.clone());
    }

    for node in file_nodes {
        let entry = map.entry(node.name.clone()).or_insert_with(|| MergedNode {
            name: node.name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            children: Vec::new(),
        });
        if node.is_dir() {
            entry.is_dir = true;
        }
    }

    let mut result: Vec<MergedNode> = map.into_values().collect();
    result.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_test_tree(nodes: Vec<FileNode>) -> FileTree {
        FileTree {
            root: PathBuf::from("/test"),
            nodes,
        }
    }

    #[test]
    fn test_initial_focus_is_file_tree() {
        let state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );
        assert_eq!(state.focus, Focus::FileTree);
    }

    #[test]
    fn test_toggle_focus() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );
        assert_eq!(state.focus, Focus::FileTree);
        state.toggle_focus();
        assert_eq!(state.focus, Focus::DiffView);
        state.toggle_focus();
        assert_eq!(state.focus, Focus::FileTree);
    }

    #[test]
    fn test_cache_update_on_select() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        // キャッシュにコンテンツを追加
        state.local_cache.insert("test.txt".to_string(), "hello\n".to_string());
        state.remote_cache.insert("test.txt".to_string(), "world\n".to_string());

        // ファイルを選択
        state.tree_cursor = 0;
        state.select_file();

        assert!(state.current_diff.is_some());
        assert_eq!(state.selected_path, Some("test.txt".to_string()));
    }

    #[test]
    fn test_clear_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );
        state.local_cache.insert("a".to_string(), "x".to_string());
        state.remote_cache.insert("b".to_string(), "y".to_string());
        state.clear_cache();
        assert!(state.local_cache.is_empty());
        assert!(state.remote_cache.is_empty());
        assert!(state.current_diff.is_none());
    }

    #[test]
    fn test_tree_expand_management() {
        let local_nodes = vec![
            FileNode::new_dir_with_children("src", vec![
                FileNode::new_file("main.rs"),
            ]),
            FileNode::new_file("README.md"),
        ];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(vec![]),
            "develop".to_string(),
        );

        // 初期状態: src と README.md の2行
        assert_eq!(state.flat_nodes.len(), 2);

        // src を展開
        state.tree_cursor = 0; // src
        state.toggle_expand();
        assert!(state.expanded_dirs.contains("src"));
        // src, main.rs, README.md の3行
        assert_eq!(state.flat_nodes.len(), 3);

        // src を折りたたみ
        state.tree_cursor = 0;
        state.toggle_expand();
        assert!(!state.expanded_dirs.contains("src"));
        assert_eq!(state.flat_nodes.len(), 2);
    }
}
