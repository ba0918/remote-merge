//! TUI アプリケーション状態管理。
//! ツリー、diff、フォーカス、コンテンツキャッシュを一元管理する。

pub mod badge;
pub mod cache;
pub mod clipboard;
pub mod clipboard_write;
pub mod dialog_ops;
pub mod diff_search;
pub mod hunk_ops;
pub mod merge_collect;
pub mod navigation;
pub mod ref_swap;
pub mod report;
pub mod scan;
pub mod search;
pub mod selection;
pub mod server_switch;
pub mod side;
pub mod three_way;
pub mod tree_ops;
pub mod types;
pub mod undo;

use std::collections::{HashMap, HashSet, VecDeque};

use cache::{BoundedCache, MAX_BINARY_CACHE_ENTRIES, MAX_TEXT_CACHE_ENTRIES};

use crate::diff::engine::{DiffResult, HunkDirection};
use crate::highlight::{HighlightCache, SyntaxHighlighter};
use crate::service::types::FileStatusKind;
use crate::theme::TuiPalette;
use crate::tree::{FileNode, FileTree};
use crate::ui::dialog::DialogState;

pub use side::Side;

pub use types::{
    Badge, CacheSnapshot, DiffMode, FlatNode, Focus, MergeScanMsg, MergeScanResult, MergeScanState,
    MergedNode, ScanState,
};

/// TUI アプリケーション全体の状態
pub struct AppState {
    /// 現在のフォーカス
    pub focus: Focus,
    /// 左側ファイルツリー
    pub left_tree: FileTree,
    /// 右側ファイルツリー
    pub right_tree: FileTree,
    /// 左側の比較元
    pub left_source: Side,
    /// 右側の比較元
    pub right_source: Side,
    /// 接続中のサーバ名（right_source のヘルパー。廃止予定）
    pub server_name: String,
    /// 利用可能なサーバ名一覧
    pub available_servers: Vec<String>,
    /// 左側ファイル内容キャッシュ (パス -> 内容)
    pub left_cache: BoundedCache<String>,
    /// 右側ファイル内容キャッシュ (パス -> 内容)
    pub right_cache: BoundedCache<String>,
    /// 左側バイナリファイル情報キャッシュ (パス -> BinaryInfo)
    pub left_binary_cache: BoundedCache<crate::diff::binary::BinaryInfo>,
    /// 右側バイナリファイル情報キャッシュ (パス -> BinaryInfo)
    pub right_binary_cache: BoundedCache<crate::diff::binary::BinaryInfo>,
    /// 現在選択中の diff 結果
    pub current_diff: Option<DiffResult>,
    /// 現在選択中のファイルパス
    pub selected_path: Option<String>,
    /// フラット化されたツリー行リスト
    pub flat_nodes: Vec<FlatNode>,
    /// ツリーのカーソル位置
    pub tree_cursor: usize,
    /// ツリーのスクロールオフセット
    pub tree_scroll: usize,
    /// ツリーの表示可能行数（最後の render で記録）
    pub tree_visible_height: usize,
    /// diff ビューのスクロールオフセット（ビューポート先頭行）
    pub diff_scroll: usize,
    /// diff ビューのカーソル位置（論理行インデックス）
    pub diff_cursor: usize,
    /// diff ビューの表示可能行数（最後の render で記録）
    pub diff_visible_height: usize,
    /// 展開中ディレクトリの集合
    pub expanded_dirs: HashSet<String>,
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
    pub disabled_patterns: HashSet<String>,
    /// コンテンツ取得に失敗したパスの集合
    pub error_paths: HashSet<String>,
    /// 現在選択中のハンクインデックス（Diff View フォーカス時）
    pub hunk_cursor: usize,
    /// ハンクマージの保留状態（→/← で選択、Enter で確定）
    pub pending_hunk_merge: Option<HunkDirection>,
    /// Diff 表示モード
    pub diff_mode: DiffMode,
    /// undo スタック（適用前のキャッシュスナップショット）
    pub undo_stack: VecDeque<CacheSnapshot>,
    /// 変更ファイルフィルターモード（Shift+F で切替）
    pub diff_filter_mode: bool,
    /// 全走査の状態
    pub scan_state: ScanState,
    /// 全走査結果の左側ツリー（キャッシュ）
    pub scan_left_tree: Option<Vec<FileNode>>,
    /// 全走査結果の右側ツリー（キャッシュ）
    pub scan_right_tree: Option<Vec<FileNode>>,
    /// 全走査の差分ステータス（パス → 差分種別）
    /// service/status.rs の compute_status_from_trees で計算された結果。
    /// CLI と TUI で共通のロジックを使用する。
    pub scan_statuses: Option<HashMap<String, FileStatusKind>>,
    /// センシティブファイルパターン
    pub sensitive_patterns: Vec<String>,
    /// マージ走査の状態
    pub merge_scan_state: MergeScanState,
    /// TUI カラーパレット（テーマから導出）
    pub palette: TuiPalette,
    /// シンタックスハイライトキャッシュ（左側）
    pub highlight_cache_left: HighlightCache,
    /// シンタックスハイライトキャッシュ（右側）
    pub highlight_cache_right: HighlightCache,
    /// 現在のテーマ名
    pub theme_name: String,
    /// シンタックスハイライト有効か
    pub syntax_highlight_enabled: bool,
    /// シンタックスハイライトエンジン
    pub highlighter: SyntaxHighlighter,
    /// ファイル検索状態
    pub search_state: search::SearchState,
    /// Diff View 内テキスト検索状態
    pub diff_search_state: search::SearchState,
    /// Reference サーバ（3way diff 用）
    pub ref_source: Option<Side>,
    /// Reference サーバのファイルツリー
    pub ref_tree: Option<FileTree>,
    /// Reference サーバのファイル内容キャッシュ
    pub ref_cache: BoundedCache<String>,
    /// Reference サーバのバイナリ情報キャッシュ
    pub ref_binary_cache: BoundedCache<crate::diff::binary::BinaryInfo>,
    /// ref diff 表示中フラグ（left/right Equal + ref 差分あり時に自動セット）
    pub showing_ref_diff: bool,
}

impl AppState {
    /// 新しい AppState を構築する。
    /// `theme_name` で初期テーマを指定する。
    pub fn new(
        left_tree: FileTree,
        right_tree: FileTree,
        left_source: Side,
        right_source: Side,
        theme_name: &str,
    ) -> Self {
        let theme = crate::theme::load_theme(theme_name);
        let palette = TuiPalette::from_theme(&theme);
        let highlighter = SyntaxHighlighter::new(theme);

        let server_name = right_source.display_name().to_string();
        let label = side::comparison_label(&left_source, &right_source);

        let mut state = Self {
            focus: Focus::FileTree,
            left_tree,
            right_tree,
            left_source,
            right_source,
            status_message: format!("{} | Tab: switch focus | q: quit", label),
            server_name,
            available_servers: Vec::new(),
            left_cache: BoundedCache::new(MAX_TEXT_CACHE_ENTRIES),
            right_cache: BoundedCache::new(MAX_TEXT_CACHE_ENTRIES),
            left_binary_cache: BoundedCache::new(MAX_BINARY_CACHE_ENTRIES),
            right_binary_cache: BoundedCache::new(MAX_BINARY_CACHE_ENTRIES),
            current_diff: None,
            selected_path: None,
            flat_nodes: Vec::new(),
            tree_cursor: 0,
            tree_scroll: 0,
            tree_visible_height: 20,
            diff_scroll: 0,
            diff_cursor: 0,
            diff_visible_height: 20,
            expanded_dirs: HashSet::new(),
            should_quit: false,
            dialog: DialogState::None,
            is_connected: false,
            exclude_patterns: Vec::new(),
            disabled_patterns: HashSet::new(),
            error_paths: HashSet::new(),
            hunk_cursor: 0,
            pending_hunk_merge: None,
            diff_mode: DiffMode::Unified,
            undo_stack: VecDeque::new(),
            diff_filter_mode: false,
            scan_state: ScanState::default(),
            scan_left_tree: None,
            scan_right_tree: None,
            scan_statuses: None,
            sensitive_patterns: Vec::new(),
            merge_scan_state: MergeScanState::default(),
            palette,
            highlight_cache_left: HighlightCache::new(),
            highlight_cache_right: HighlightCache::new(),
            theme_name: theme_name.to_string(),
            syntax_highlight_enabled: true,
            highlighter,
            search_state: search::SearchState::default(),
            diff_search_state: search::SearchState::default(),
            ref_source: None,
            ref_tree: None,
            ref_cache: BoundedCache::new(MAX_TEXT_CACHE_ENTRIES),
            ref_binary_cache: BoundedCache::new(MAX_BINARY_CACHE_ENTRIES),
            showing_ref_diff: false,
        };
        state.rebuild_flat_nodes();
        state
    }

    /// 両サイドがリモート同士か（remote ↔ remote 比較モード）
    pub fn is_remote_to_remote(&self) -> bool {
        self.left_source.is_remote() && self.right_source.is_remote()
    }

    /// reference サーバが設定されているか
    pub fn has_reference(&self) -> bool {
        self.ref_source.is_some()
    }

    /// reference サーバの表示名を返す
    pub fn ref_server_name(&self) -> Option<&str> {
        self.ref_source.as_ref().map(|s| s.display_name())
    }

    /// reference サーバを設定する
    pub fn set_reference(&mut self, source: Side, tree: FileTree) {
        self.ref_source = Some(source);
        self.ref_tree = Some(tree);
        self.ref_cache.clear();
        self.ref_binary_cache.clear();
    }

    /// ref diff 表示中かどうか
    pub fn is_showing_ref_diff(&self) -> bool {
        self.showing_ref_diff
    }

    /// reference サーバをクリアする
    pub fn clear_reference(&mut self) {
        self.ref_source = None;
        self.ref_tree = None;
        self.ref_cache.clear();
        self.ref_binary_cache.clear();
    }
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
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.focus, Focus::FileTree);
    }

    #[test]
    fn test_is_remote_to_remote() {
        let state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Remote("develop".to_string()),
            Side::Remote("staging".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert!(state.is_remote_to_remote());
    }

    #[test]
    fn test_ref_fields_initially_none() {
        let state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert!(!state.has_reference());
        assert!(state.ref_source.is_none());
        assert!(state.ref_tree.is_none());
        assert!(state.ref_cache.is_empty());
        assert!(state.ref_binary_cache.is_empty());
    }

    #[test]
    fn test_set_reference() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        let ref_tree = make_test_tree(vec![FileNode::new_file("x.rs")]);
        state.set_reference(Side::Remote("staging".to_string()), ref_tree);
        assert!(state.has_reference());
        assert_eq!(state.ref_server_name(), Some("staging"));
        assert!(state.ref_tree.is_some());
    }

    #[test]
    fn test_clear_reference() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.set_reference(Side::Remote("staging".to_string()), make_test_tree(vec![]));
        assert!(state.has_reference());
        state.clear_reference();
        assert!(!state.has_reference());
        assert!(state.ref_source.is_none());
        assert!(state.ref_tree.is_none());
    }

    #[test]
    fn test_ref_server_name_local() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Remote("develop".to_string()),
            Side::Remote("staging".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        state.set_reference(Side::Local, make_test_tree(vec![]));
        assert_eq!(state.ref_server_name(), Some("local"));
    }

    #[test]
    fn test_is_not_remote_to_remote() {
        let state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            Side::Local,
            Side::Remote("develop".to_string()),
            crate::theme::DEFAULT_THEME,
        );
        assert!(!state.is_remote_to_remote());
    }
}
