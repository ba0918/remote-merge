//! TUI アプリケーション状態管理。
//! ツリー、diff、フォーカス、コンテンツキャッシュを一元管理する。

use std::collections::HashMap;
use std::path::Path;

use crate::diff::engine::{self, DiffHunk, DiffResult, HunkDirection};
use crate::merge::executor::MergeDirection;
use crate::tree::{FileNode, FileTree};
use crate::ui::dialog::{ConfirmDialog, DialogState, FilterPanel, HelpOverlay, ServerMenu};

/// TUI のフォーカス対象
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    FileTree,
    DiffView,
}

/// Diff 表示モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffMode {
    /// 統一形式 (Unified)
    Unified,
    /// 左右比較 (Side-by-Side)
    SideBySide,
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
    /// 利用可能なサーバ名一覧
    pub available_servers: Vec<String>,
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
    /// ダイアログ状態
    pub dialog: DialogState,
    /// SSH 接続済みか
    pub is_connected: bool,
    /// 除外フィルターパターン（元の設定値）
    pub exclude_patterns: Vec<String>,
    /// 一時的に無効化されたパターン
    pub disabled_patterns: std::collections::HashSet<String>,
    /// 現在選択中のハンクインデックス（Diff View フォーカス時）
    pub hunk_cursor: usize,
    /// ハンクマージの保留状態（→/← で選択、Enter で確定）
    pub pending_hunk_merge: Option<HunkDirection>,
    /// Diff 表示モード
    pub diff_mode: DiffMode,
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
            available_servers: Vec::new(),
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
            dialog: DialogState::None,
            is_connected: false,
            exclude_patterns: Vec::new(),
            disabled_patterns: std::collections::HashSet::new(),
            hunk_cursor: 0,
            pending_hunk_merge: None,
            diff_mode: DiffMode::Unified,
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

    /// Diff 表示モードを切り替える (d キー)
    pub fn toggle_diff_mode(&mut self) {
        self.diff_mode = match self.diff_mode {
            DiffMode::Unified => DiffMode::SideBySide,
            DiffMode::SideBySide => DiffMode::Unified,
        };
    }

    /// diff の全行数を返す
    pub fn diff_line_count(&self) -> usize {
        match &self.current_diff {
            Some(DiffResult::Modified { lines, .. }) => lines.len(),
            _ => 0,
        }
    }

    /// ページ下スクロール
    pub fn scroll_page_down(&mut self, page_size: usize) {
        let max = self.diff_line_count().saturating_sub(1);
        self.diff_scroll = (self.diff_scroll + page_size).min(max);
    }

    /// ページ上スクロール
    pub fn scroll_page_up(&mut self, page_size: usize) {
        self.diff_scroll = self.diff_scroll.saturating_sub(page_size);
    }

    /// 先頭にスクロール
    pub fn scroll_to_home(&mut self) {
        self.diff_scroll = 0;
    }

    /// 末尾にスクロール
    pub fn scroll_to_end(&mut self) {
        self.diff_scroll = self.diff_line_count().saturating_sub(1);
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
        self.hunk_cursor = 0;
        self.pending_hunk_merge = None;
    }

    /// ハンクマージの保留をセットする（→/← で呼ぶ）
    pub fn stage_hunk_merge(&mut self, direction: HunkDirection) {
        if self.hunk_count() == 0 {
            return;
        }
        self.pending_hunk_merge = Some(direction);
        let dir_str = match direction {
            HunkDirection::RightToLeft => "remote → local",
            HunkDirection::LeftToRight => "local → remote",
        };
        self.status_message = format!(
            "Hunk {}/{} ({}) — Enter: apply / Esc: cancel",
            self.hunk_cursor + 1,
            self.hunk_count(),
            dir_str,
        );
    }

    /// 保留中のハンクマージをキャンセルする
    pub fn cancel_hunk_merge(&mut self) {
        if self.pending_hunk_merge.is_some() {
            self.pending_hunk_merge = None;
            self.status_message = format!(
                "Hunk merge cancelled | hunk {}/{}",
                self.hunk_cursor + 1,
                self.hunk_count(),
            );
        }
    }

    /// マージ確認ダイアログを表示する (Shift+L / Shift+R)
    pub fn show_merge_dialog(&mut self, direction: MergeDirection) {
        let node = match self.flat_nodes.get(self.tree_cursor) {
            Some(n) if !n.is_dir => n.clone(),
            _ => {
                self.status_message = "ファイルを選択してください".to_string();
                return;
            }
        };

        let (source, target) = match direction {
            MergeDirection::LeftMerge => ("local".to_string(), self.server_name.clone()),
            MergeDirection::RightMerge => (self.server_name.clone(), "local".to_string()),
        };

        self.dialog = DialogState::Confirm(ConfirmDialog::new(
            node.path.clone(),
            direction,
            source,
            target,
        ));
    }

    /// サーバ選択メニューを表示する (s キー)
    pub fn show_server_menu(&mut self) {
        if self.available_servers.is_empty() {
            self.status_message = "利用可能なサーバがありません".to_string();
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
    pub fn update_badge_after_merge(&mut self, path: &str, content: &str, direction: MergeDirection) {
        match direction {
            MergeDirection::LeftMerge => {
                // ローカル → リモート: リモートキャッシュをローカルの内容で更新
                self.remote_cache.insert(path.to_string(), content.to_string());
            }
            MergeDirection::RightMerge => {
                // リモート → ローカル: ローカルキャッシュをリモートの内容で更新
                self.local_cache.insert(path.to_string(), content.to_string());
            }
        }
        // diff を再計算
        if self.selected_path.as_deref() == Some(path) {
            self.select_file();
        }
        self.rebuild_flat_nodes();
    }

    /// サーバ切替後にツリーを再構築する
    pub fn switch_server(&mut self, new_server: String, remote_tree: FileTree) {
        self.server_name = new_server.clone();
        self.remote_tree = remote_tree;
        self.remote_cache.clear();
        self.current_diff = None;
        self.selected_path = None;
        self.diff_scroll = 0;
        self.rebuild_flat_nodes();
        self.status_message = format!(
            "local ↔ {} | Tab: switch focus | q: quit",
            new_server
        );
        self.is_connected = true;
    }

    /// フィルターパネルを表示する (f キー)
    pub fn show_filter_panel(&mut self) {
        if self.exclude_patterns.is_empty() {
            self.status_message = "除外パターンが設定されていません".to_string();
            return;
        }
        let mut panel = FilterPanel::new(&self.exclude_patterns);
        // 無効化済みパターンを反映
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
                self.status_message = format!("ローカルディレクトリ取得失敗: {}", e);
            }
        }
    }

    /// ディレクトリのリフレッシュ（子ノードをクリア）
    pub fn refresh_directory(&mut self, path: &str) {
        // ローカルツリーの子ノードをクリア
        if let Some(node) = self.local_tree.find_node_mut(std::path::Path::new(path)) {
            node.children = None;
        }
        // リモートツリーの子ノードをクリア
        if let Some(node) = self.remote_tree.find_node_mut(std::path::Path::new(path)) {
            node.children = None;
        }
        self.rebuild_flat_nodes();
        self.status_message = format!("{}: リフレッシュしました", path);
    }

    /// 現在カーソル位置のパスを返す
    pub fn current_path(&self) -> Option<String> {
        self.flat_nodes.get(self.tree_cursor).map(|n| n.path.clone())
    }

    /// 現在カーソル位置がディレクトリかどうか
    pub fn current_is_dir(&self) -> bool {
        self.flat_nodes
            .get(self.tree_cursor)
            .is_some_and(|n| n.is_dir)
    }

    /// 現在の diff のハンク数を返す
    pub fn hunk_count(&self) -> usize {
        match &self.current_diff {
            Some(DiffResult::Modified { merge_hunks, .. }) => merge_hunks.len(),
            _ => 0,
        }
    }

    /// ハンクカーソルを上に移動（前のハンクへ）
    pub fn hunk_cursor_up(&mut self) {
        if self.hunk_cursor > 0 {
            self.hunk_cursor -= 1;
            self.scroll_to_hunk();
        }
    }

    /// ハンクカーソルを下に移動（次のハンクへ）
    pub fn hunk_cursor_down(&mut self) {
        let count = self.hunk_count();
        if count > 0 && self.hunk_cursor + 1 < count {
            self.hunk_cursor += 1;
            self.scroll_to_hunk();
        }
    }

    /// ハンクカーソル位置に diff_scroll を合わせる
    fn scroll_to_hunk(&mut self) {
        if let Some(DiffResult::Modified { merge_hunks, lines, .. }) = &self.current_diff {
            if let Some(hunk) = merge_hunks.get(self.hunk_cursor) {
                // ハンクの先頭行が全体 lines 内の何行目かを探す
                if let Some(first_hunk_line) = hunk.lines.first() {
                    let scroll_target = lines
                        .iter()
                        .position(|l| std::ptr::eq(l, first_hunk_line))
                        .unwrap_or_else(|| {
                            // ポインタ比較が失敗した場合はインデックスベースで探す
                            self.find_hunk_start_in_lines(lines, hunk)
                        });
                    self.diff_scroll = scroll_target;
                }
            }
        }
    }

    /// ハンクの開始位置を lines 内で探す（内容ベース）
    fn find_hunk_start_in_lines(
        &self,
        lines: &[engine::DiffLine],
        hunk: &DiffHunk,
    ) -> usize {
        if hunk.lines.is_empty() {
            return 0;
        }
        let first = &hunk.lines[0];
        for (i, line) in lines.iter().enumerate() {
            if line.tag == first.tag
                && line.value == first.value
                && line.old_index == first.old_index
                && line.new_index == first.new_index
            {
                return i;
            }
        }
        0
    }

    /// ハンクマージのプレビューテキスト（before/after）を生成する
    pub fn preview_hunk_merge(&self, direction: HunkDirection) -> Option<(String, String)> {
        let path = self.selected_path.as_ref()?;

        let (hunks, _lines) = match &self.current_diff {
            Some(DiffResult::Modified { merge_hunks, lines, .. }) => (merge_hunks.clone(), lines.clone()),
            _ => return None,
        };

        let hunk = hunks.get(self.hunk_cursor)?;

        // 適用先テキストを取得
        let original = match direction {
            HunkDirection::RightToLeft => self.local_cache.get(path)?.clone(),
            HunkDirection::LeftToRight => self.remote_cache.get(path)?.clone(),
        };

        let new_text = engine::apply_hunk_to_text(&original, hunk, direction);

        Some((original, new_text))
    }

    /// ハンク単位マージを実行する
    ///
    /// direction: RightToLeft なら right の変更を left に取り込む
    ///            LeftToRight なら left の変更を right に取り込む
    pub fn apply_hunk_merge(&mut self, direction: HunkDirection) -> Option<String> {
        let path = self.selected_path.clone()?;

        let (hunks, _lines) = match &self.current_diff {
            Some(DiffResult::Modified { merge_hunks, lines, .. }) => (merge_hunks.clone(), lines.clone()),
            _ => return None,
        };

        let hunk = hunks.get(self.hunk_cursor)?;

        // 適用先テキストを取得
        let original = match direction {
            HunkDirection::RightToLeft => self.local_cache.get(&path)?.clone(),
            HunkDirection::LeftToRight => self.remote_cache.get(&path)?.clone(),
        };

        let new_text = engine::apply_hunk_to_text(&original, hunk, direction);

        // キャッシュを更新
        match direction {
            HunkDirection::RightToLeft => {
                self.local_cache.insert(path.clone(), new_text.clone());
            }
            HunkDirection::LeftToRight => {
                self.remote_cache.insert(path.clone(), new_text.clone());
            }
        }

        // diff を再計算
        let local = self.local_cache.get(&path);
        let remote = self.remote_cache.get(&path);
        if let (Some(l), Some(r)) = (local, remote) {
            self.current_diff = Some(engine::compute_diff(l, r));
        }

        // ハンクカーソルを範囲内に収める
        let new_count = self.hunk_count();
        if new_count == 0 {
            self.hunk_cursor = 0;
        } else if self.hunk_cursor >= new_count {
            self.hunk_cursor = new_count - 1;
        }

        // バッジを再構築
        self.rebuild_flat_nodes();

        let dir_str = match direction {
            HunkDirection::RightToLeft => "right → left",
            HunkDirection::LeftToRight => "left → right",
        };
        self.status_message = format!(
            "Hunk {} applied ({}) | {} hunks remaining",
            self.hunk_cursor + 1,
            dir_str,
            self.hunk_count(),
        );

        Some(path)
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
    fn test_show_merge_dialog_left() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftMerge);
        assert!(matches!(state.dialog, DialogState::Confirm(_)));

        if let DialogState::Confirm(ref d) = state.dialog {
            assert_eq!(d.file_path, "test.txt");
            assert_eq!(d.direction, MergeDirection::LeftMerge);
        }
    }

    #[test]
    fn test_show_merge_dialog_right() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::RightMerge);

        if let DialogState::Confirm(ref d) = state.dialog {
            assert_eq!(d.direction, MergeDirection::RightMerge);
        } else {
            panic!("Expected Confirm dialog");
        }
    }

    #[test]
    fn test_show_merge_dialog_dir_skipped() {
        let local_nodes = vec![FileNode::new_dir("src")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(vec![]),
            "develop".to_string(),
        );

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftMerge);
        assert!(matches!(state.dialog, DialogState::None));
    }

    #[test]
    fn test_server_menu() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );
        state.available_servers = vec![
            "develop".to_string(),
            "staging".to_string(),
        ];

        state.show_server_menu();
        assert!(matches!(state.dialog, DialogState::ServerSelect(_)));
    }

    #[test]
    fn test_close_dialog() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LeftMerge);
        assert!(state.has_dialog());
        state.close_dialog();
        assert!(!state.has_dialog());
    }

    #[test]
    fn test_switch_server() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );
        state.remote_cache.insert("a.txt".to_string(), "old".to_string());

        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);

        assert_eq!(state.server_name, "staging");
        assert!(state.remote_cache.is_empty());
        assert!(state.is_connected);
    }

    #[test]
    fn test_update_badge_after_merge() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.local_cache.insert("test.txt".to_string(), "content".to_string());
        state.update_badge_after_merge("test.txt", "content", MergeDirection::LeftMerge);

        // リモートキャッシュが更新されている
        assert_eq!(state.remote_cache.get("test.txt").unwrap(), "content");
    }

    #[test]
    fn test_hunk_cursor_navigation() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        // 離れた2箇所に変更がある diff を設定
        let old: String = (0..20).map(|i| format!("line{}\n", i)).collect();
        let mut new_text = old.clone();
        new_text = new_text.replace("line3\n", "modified3\n");
        new_text = new_text.replace("line15\n", "modified15\n");

        state.local_cache.insert("test.txt".to_string(), old);
        state.remote_cache.insert("test.txt".to_string(), new_text);
        state.tree_cursor = 0;
        state.select_file();

        assert_eq!(state.hunk_count(), 2);
        assert_eq!(state.hunk_cursor, 0);

        state.hunk_cursor_down();
        assert_eq!(state.hunk_cursor, 1);

        state.hunk_cursor_down(); // 境界: 動かない
        assert_eq!(state.hunk_cursor, 1);

        state.hunk_cursor_up();
        assert_eq!(state.hunk_cursor, 0);

        state.hunk_cursor_up(); // 境界: 動かない
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_hunk_cursor_bounds() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );

        // diff なしの状態
        assert_eq!(state.hunk_count(), 0);
        state.hunk_cursor_down();
        assert_eq!(state.hunk_cursor, 0);
        state.hunk_cursor_up();
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_hunk_merge_updates_cache() {
        use crate::diff::engine::HunkDirection;

        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.local_cache.insert("test.txt".to_string(), "line1\nline2\nline3\n".to_string());
        state.remote_cache.insert("test.txt".to_string(), "line1\nmodified\nline3\n".to_string());
        state.selected_path = Some("test.txt".to_string());
        state.tree_cursor = 0;
        state.select_file();

        // RightToLeft: remote の modified を local に取り込む
        let result = state.apply_hunk_merge(HunkDirection::RightToLeft);
        assert!(result.is_some());
        assert_eq!(
            state.local_cache.get("test.txt").unwrap(),
            "line1\nmodified\nline3\n"
        );
    }

    #[test]
    fn test_hunk_merge_recalculates_diff() {
        use crate::diff::engine::HunkDirection;

        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.local_cache.insert("test.txt".to_string(), "a\nb\nc\n".to_string());
        state.remote_cache.insert("test.txt".to_string(), "a\nX\nc\n".to_string());
        state.selected_path = Some("test.txt".to_string());
        state.tree_cursor = 0;
        state.select_file();

        assert_eq!(state.hunk_count(), 1);

        // ハンクマージ実行
        state.apply_hunk_merge(HunkDirection::RightToLeft);

        // マージ後は local == remote なので Equal になるはず
        match &state.current_diff {
            Some(DiffResult::Equal) => {} // OK
            other => panic!("Equal を期待したが {:?}", other),
        }
        assert_eq!(state.hunk_count(), 0);
    }

    #[test]
    fn test_stage_hunk_merge_sets_pending() {
        use crate::diff::engine::HunkDirection;

        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.local_cache.insert("test.txt".to_string(), "a\nb\nc\n".to_string());
        state.remote_cache.insert("test.txt".to_string(), "a\nX\nc\n".to_string());
        state.tree_cursor = 0;
        state.select_file();

        assert!(state.pending_hunk_merge.is_none());

        // → で RightToLeft を選択
        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert_eq!(state.pending_hunk_merge, Some(HunkDirection::RightToLeft));
        assert!(state.status_message.contains("Enter"));
        assert!(state.status_message.contains("Esc"));

        // ← で上書き
        state.stage_hunk_merge(HunkDirection::LeftToRight);
        assert_eq!(state.pending_hunk_merge, Some(HunkDirection::LeftToRight));
    }

    #[test]
    fn test_cancel_hunk_merge_clears_pending() {
        use crate::diff::engine::HunkDirection;

        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.local_cache.insert("test.txt".to_string(), "a\nb\nc\n".to_string());
        state.remote_cache.insert("test.txt".to_string(), "a\nX\nc\n".to_string());
        state.tree_cursor = 0;
        state.select_file();

        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert!(state.pending_hunk_merge.is_some());

        state.cancel_hunk_merge();
        assert!(state.pending_hunk_merge.is_none());
        assert!(state.status_message.contains("cancelled"));
    }

    #[test]
    fn test_stage_hunk_merge_noop_when_no_hunks() {
        use crate::diff::engine::HunkDirection;

        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
        );

        // diff なし → stage しても pending にならない
        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert!(state.pending_hunk_merge.is_none());
    }

    #[test]
    fn test_select_file_clears_pending() {
        use crate::diff::engine::HunkDirection;

        let local_nodes = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];
        let remote_nodes = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
        );

        state.local_cache.insert("a.txt".to_string(), "old\n".to_string());
        state.remote_cache.insert("a.txt".to_string(), "new\n".to_string());
        state.local_cache.insert("b.txt".to_string(), "x\n".to_string());
        state.remote_cache.insert("b.txt".to_string(), "y\n".to_string());

        state.tree_cursor = 0;
        state.select_file();
        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert!(state.pending_hunk_merge.is_some());

        // 別ファイル選択 → pending がクリアされる
        state.tree_cursor = 1;
        state.select_file();
        assert!(state.pending_hunk_merge.is_none());
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
