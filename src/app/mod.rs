//! TUI アプリケーション状態管理。
//! ツリー、diff、フォーカス、コンテンツキャッシュを一元管理する。

pub mod badge;
pub mod dialog_ops;
pub mod hunk_ops;
pub mod merge_collect;
pub mod navigation;
pub mod scan;
pub mod search;
pub mod selection;
pub mod tree_ops;
pub mod types;

use std::collections::{HashMap, HashSet, VecDeque};

use crate::diff::engine::{DiffResult, HunkDirection};
use crate::highlight::{HighlightCache, SyntaxHighlighter};
use crate::theme::TuiPalette;
use crate::tree::{FileNode, FileTree};
use crate::ui::dialog::DialogState;

pub use types::{
    Badge, CacheSnapshot, DiffMode, FlatNode, Focus, MergeScanMsg, MergeScanResult, MergeScanState,
    MergedNode, ScanState,
};

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
    /// ローカルファイル内容キャッシュ (パス -> 内容)
    pub local_cache: HashMap<String, String>,
    /// リモートファイル内容キャッシュ (パス -> 内容)
    pub remote_cache: HashMap<String, String>,
    /// ローカルバイナリファイル情報キャッシュ (パス -> BinaryInfo)
    pub local_binary_cache: HashMap<String, crate::diff::binary::BinaryInfo>,
    /// リモートバイナリファイル情報キャッシュ (パス -> BinaryInfo)
    pub remote_binary_cache: HashMap<String, crate::diff::binary::BinaryInfo>,
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
    /// 全走査結果のローカルツリー（キャッシュ）
    pub scan_local_tree: Option<Vec<FileNode>>,
    /// 全走査結果のリモートツリー（キャッシュ）
    pub scan_remote_tree: Option<Vec<FileNode>>,
    /// センシティブファイルパターン
    pub sensitive_patterns: Vec<String>,
    /// マージ走査の状態
    pub merge_scan_state: MergeScanState,
    /// TUI カラーパレット（テーマから導出）
    pub palette: TuiPalette,
    /// シンタックスハイライトキャッシュ（ローカル側）
    pub highlight_cache_local: HighlightCache,
    /// シンタックスハイライトキャッシュ（リモート側）
    pub highlight_cache_remote: HighlightCache,
    /// 現在のテーマ名
    pub theme_name: String,
    /// シンタックスハイライト有効か
    pub syntax_highlight_enabled: bool,
    /// シンタックスハイライトエンジン
    pub highlighter: SyntaxHighlighter,
    /// ファイル検索状態
    pub search_state: search::SearchState,
}

impl AppState {
    /// 新しい AppState を構築する。
    /// `theme_name` で初期テーマを指定する。
    pub fn new(
        local_tree: FileTree,
        remote_tree: FileTree,
        server_name: String,
        theme_name: &str,
    ) -> Self {
        let theme = crate::theme::load_theme(theme_name);
        let palette = TuiPalette::from_theme(&theme);
        let highlighter = SyntaxHighlighter::new(theme);

        let mut state = Self {
            focus: Focus::FileTree,
            local_tree,
            remote_tree,
            status_message: format!("local <-> {} | Tab: switch focus | q: quit", &server_name),
            server_name,
            available_servers: Vec::new(),
            local_cache: HashMap::new(),
            remote_cache: HashMap::new(),
            local_binary_cache: HashMap::new(),
            remote_binary_cache: HashMap::new(),
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
            scan_local_tree: None,
            scan_remote_tree: None,
            sensitive_patterns: Vec::new(),
            merge_scan_state: MergeScanState::default(),
            palette,
            highlight_cache_local: HighlightCache::new(),
            highlight_cache_remote: HighlightCache::new(),
            theme_name: theme_name.to_string(),
            syntax_highlight_enabled: true,
            highlighter,
            search_state: search::SearchState::default(),
        };
        state.rebuild_flat_nodes();
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::engine::HunkDirection;
    use crate::merge::executor::MergeDirection;
    use crate::ui::dialog::BatchConfirmDialog;
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
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.focus, Focus::FileTree);
    }

    #[test]
    fn test_toggle_focus() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
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
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "hello\n".to_string());
        state
            .remote_cache
            .insert("test.txt".to_string(), "world\n".to_string());

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
            crate::theme::DEFAULT_THEME,
        );
        state.local_cache.insert("a".to_string(), "x".to_string());
        state.remote_cache.insert("b".to_string(), "y".to_string());
        state.error_paths.insert("some/path".to_string());
        state.clear_cache();
        assert!(state.local_cache.is_empty());
        assert!(state.remote_cache.is_empty());
        assert!(state.current_diff.is_none());
        assert!(state.error_paths.is_empty());
    }

    #[test]
    fn test_show_merge_dialog_left() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(matches!(state.dialog, DialogState::Confirm(_)));

        if let DialogState::Confirm(ref d) = state.dialog {
            assert_eq!(d.file_path, "test.txt");
            assert_eq!(d.direction, MergeDirection::LocalToRemote);
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
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::RemoteToLocal);

        if let DialogState::Confirm(ref d) = state.dialog {
            assert_eq!(d.direction, MergeDirection::RemoteToLocal);
        } else {
            panic!("Expected Confirm dialog");
        }
    }

    #[test]
    fn test_show_merge_dialog_file_equal_shows_info() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        // 両方同じ内容 → Equal
        state
            .local_cache
            .insert("test.txt".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("test.txt".to_string(), "same".to_string());

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(
            matches!(state.dialog, DialogState::Info(_)),
            "Expected Info dialog for equal file, got {:?}",
            state.dialog
        );
    }

    #[test]
    fn test_show_merge_dialog_dir_skipped() {
        let local_nodes = vec![FileNode::new_dir("src")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(matches!(state.dialog, DialogState::None));
    }

    #[test]
    fn test_server_menu() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state.available_servers = vec!["develop".to_string(), "staging".to_string()];

        state.show_server_menu();
        assert!(matches!(state.dialog, DialogState::ServerSelect(_)));
    }

    #[test]
    fn test_close_dialog() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
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
            crate::theme::DEFAULT_THEME,
        );
        state
            .remote_cache
            .insert("a.txt".to_string(), "old".to_string());

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
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "content".to_string());
        state.update_badge_after_merge("test.txt", "content", MergeDirection::LocalToRemote);

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
            crate::theme::DEFAULT_THEME,
        );

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

        state.hunk_cursor_down();
        assert_eq!(state.hunk_cursor, 1);

        state.hunk_cursor_up();
        assert_eq!(state.hunk_cursor, 0);

        state.hunk_cursor_up();
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_hunk_cursor_bounds() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        assert_eq!(state.hunk_count(), 0);
        state.hunk_cursor_down();
        assert_eq!(state.hunk_cursor, 0);
        state.hunk_cursor_up();
        assert_eq!(state.hunk_cursor, 0);
    }

    #[test]
    fn test_hunk_merge_updates_cache() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "line1\nline2\nline3\n".to_string());
        state.remote_cache.insert(
            "test.txt".to_string(),
            "line1\nmodified\nline3\n".to_string(),
        );
        state.selected_path = Some("test.txt".to_string());
        state.tree_cursor = 0;
        state.select_file();

        let result = state.apply_hunk_merge(HunkDirection::RightToLeft);
        assert!(result.is_some());
        assert_eq!(
            state.local_cache.get("test.txt").unwrap(),
            "line1\nmodified\nline3\n"
        );
    }

    #[test]
    fn test_hunk_merge_recalculates_diff() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "a\nb\nc\n".to_string());
        state
            .remote_cache
            .insert("test.txt".to_string(), "a\nX\nc\n".to_string());
        state.selected_path = Some("test.txt".to_string());
        state.tree_cursor = 0;
        state.select_file();

        assert_eq!(state.hunk_count(), 1);

        state.apply_hunk_merge(HunkDirection::RightToLeft);

        match &state.current_diff {
            Some(DiffResult::Equal) => {}
            other => panic!("Expected Equal but got {:?}", other),
        }
        assert_eq!(state.hunk_count(), 0);
    }

    #[test]
    fn test_stage_hunk_merge_sets_pending() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "a\nb\nc\n".to_string());
        state
            .remote_cache
            .insert("test.txt".to_string(), "a\nX\nc\n".to_string());
        state.tree_cursor = 0;
        state.select_file();

        assert!(state.pending_hunk_merge.is_none());

        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert_eq!(state.pending_hunk_merge, Some(HunkDirection::RightToLeft));
        assert!(state.status_message.contains("Enter"));
        assert!(state.status_message.contains("Esc"));

        state.stage_hunk_merge(HunkDirection::LeftToRight);
        assert_eq!(state.pending_hunk_merge, Some(HunkDirection::LeftToRight));
    }

    #[test]
    fn test_cancel_hunk_merge_clears_pending() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "a\nb\nc\n".to_string());
        state
            .remote_cache
            .insert("test.txt".to_string(), "a\nX\nc\n".to_string());
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
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert!(state.pending_hunk_merge.is_none());
    }

    #[test]
    fn test_select_file_clears_pending() {
        let local_nodes = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];
        let remote_nodes = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("a.txt".to_string(), "old\n".to_string());
        state
            .remote_cache
            .insert("a.txt".to_string(), "new\n".to_string());
        state
            .local_cache
            .insert("b.txt".to_string(), "x\n".to_string());
        state
            .remote_cache
            .insert("b.txt".to_string(), "y\n".to_string());

        state.tree_cursor = 0;
        state.select_file();
        state.stage_hunk_merge(HunkDirection::RightToLeft);
        assert!(state.pending_hunk_merge.is_some());

        state.tree_cursor = 1;
        state.select_file();
        assert!(state.pending_hunk_merge.is_none());
    }

    #[test]
    fn test_show_merge_dialog_dir_opens_batch_confirm() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.is_connected = true;

        state.tree_cursor = 0;
        state.toggle_expand();
        state
            .local_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "new".to_string());
        state
            .local_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        state.rebuild_flat_nodes();

        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);

        match &state.dialog {
            DialogState::BatchConfirm(batch) => {
                assert_eq!(batch.files.len(), 1);
                assert_eq!(batch.files[0].0, "src/a.ts");
                assert_eq!(batch.files[0].1, Badge::Modified);
            }
            other => panic!("Expected BatchConfirm but got {:?}", other),
        }
    }

    #[test]
    fn test_collect_diff_files_under_empty() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.toggle_expand();
        state
            .local_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state.rebuild_flat_nodes();

        let (files, _) = state.collect_diff_files_under("src");
        assert!(files.is_empty());

        state.is_connected = true;
        state.tree_cursor = 0;
        state.show_merge_dialog(MergeDirection::LocalToRemote);
        assert!(matches!(state.dialog, DialogState::Info(_)));
    }

    #[test]
    fn test_collect_diff_files_unchecked_dirs() {
        // nested は両方に存在＋未展開なので Unchecked
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_dir("nested")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_dir("nested")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.toggle_expand();

        let (files, unchecked) = state.collect_diff_files_under("src");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1, Badge::LocalOnly);
        assert_eq!(unchecked, 1);
    }

    #[test]
    fn test_batch_confirm_boundary_20_files() {
        let batch = BatchConfirmDialog::new(
            (0..20)
                .map(|i| (format!("file{}.txt", i), Badge::Modified))
                .collect(),
            MergeDirection::LocalToRemote,
            "local".to_string(),
            "develop".to_string(),
            0,
        );
        assert!(!batch.is_large_batch());
    }

    #[test]
    fn test_batch_confirm_boundary_21_files() {
        let batch = BatchConfirmDialog::new(
            (0..21)
                .map(|i| (format!("file{}.txt", i), Badge::Modified))
                .collect(),
            MergeDirection::LocalToRemote,
            "local".to_string(),
            "develop".to_string(),
            0,
        );
        assert!(batch.is_large_batch());
    }

    #[test]
    fn test_diff_filter_mode_hides_equal() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes.clone()),
            make_test_tree(remote_nodes.clone()),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.toggle_expand();

        state
            .local_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "new".to_string());
        state
            .local_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("src/b.ts".to_string(), "same".to_string());

        state.set_scan_result(local_nodes, remote_nodes);
        state.rebuild_flat_nodes();

        assert!(!state.diff_filter_mode);
        let all_files: Vec<&str> = state
            .flat_nodes
            .iter()
            .filter(|n| !n.is_dir)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(all_files.len(), 2);

        state.toggle_diff_filter();
        assert!(state.diff_filter_mode);

        let filtered_files: Vec<&str> = state
            .flat_nodes
            .iter()
            .filter(|n| !n.is_dir)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(filtered_files.len(), 1);
        assert_eq!(filtered_files[0], "src/a.ts");

        state.toggle_diff_filter();
        assert!(!state.diff_filter_mode);
        let all_again: Vec<&str> = state
            .flat_nodes
            .iter()
            .filter(|n| !n.is_dir)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(all_again.len(), 2);
    }

    #[test]
    fn test_diff_filter_hides_equal_dirs() {
        let local_nodes = vec![
            FileNode::new_dir_with_children("src", vec![FileNode::new_file("a.ts")]),
            FileNode::new_dir_with_children("test", vec![FileNode::new_file("t.ts")]),
        ];
        let remote_nodes = vec![
            FileNode::new_dir_with_children("src", vec![FileNode::new_file("a.ts")]),
            FileNode::new_dir_with_children("test", vec![FileNode::new_file("t.ts")]),
        ];

        let mut state = AppState::new(
            make_test_tree(local_nodes.clone()),
            make_test_tree(remote_nodes.clone()),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.toggle_expand();
        state.tree_cursor = 2;
        state.toggle_expand();

        state
            .local_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "new".to_string());
        state
            .local_cache
            .insert("test/t.ts".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("test/t.ts".to_string(), "same".to_string());

        state.set_scan_result(local_nodes, remote_nodes);

        state.toggle_diff_filter();

        let names: Vec<&str> = state.flat_nodes.iter().map(|n| n.path.as_str()).collect();
        assert!(names.contains(&"src"));
        assert!(names.contains(&"src/a.ts"));
        assert!(!names.contains(&"test"));
        assert!(!names.contains(&"test/t.ts"));
    }

    #[test]
    fn test_scan_state_default() {
        let state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        assert!(matches!(state.scan_state, ScanState::Idle));
        assert!(!state.diff_filter_mode);
        assert!(state.scan_local_tree.is_none());
    }

    #[test]
    fn test_clear_scan_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state.scan_local_tree = Some(vec![]);
        state.scan_remote_tree = Some(vec![]);
        state.diff_filter_mode = true;

        state.clear_scan_cache();
        assert!(state.scan_local_tree.is_none());
        assert!(state.scan_remote_tree.is_none());
        assert!(!state.diff_filter_mode);
    }

    #[test]
    fn test_batch_confirm_sensitive_check() {
        let mut batch = BatchConfirmDialog::new(
            vec![
                ("src/app.ts".to_string(), Badge::Modified),
                (".env".to_string(), Badge::Modified),
                ("config/secrets.json".to_string(), Badge::Modified),
                ("server.key".to_string(), Badge::LocalOnly),
            ],
            MergeDirection::LocalToRemote,
            "local".to_string(),
            "develop".to_string(),
            0,
        );

        batch.check_sensitive(&[
            ".env".into(),
            ".env.*".into(),
            "*.pem".into(),
            "*.key".into(),
            "*secret*".into(),
        ]);

        assert_eq!(batch.sensitive_files.len(), 3);
        assert!(batch.sensitive_files.contains(&".env".to_string()));
        assert!(batch
            .sensitive_files
            .contains(&"config/secrets.json".to_string()));
        assert!(batch.sensitive_files.contains(&"server.key".to_string()));
    }

    #[test]
    fn test_tree_expand_management() {
        let local_nodes = vec![
            FileNode::new_dir_with_children("src", vec![FileNode::new_file("main.rs")]),
            FileNode::new_file("README.md"),
        ];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        assert_eq!(state.flat_nodes.len(), 2);

        state.tree_cursor = 0;
        state.toggle_expand();
        assert!(state.expanded_dirs.contains("src"));
        assert_eq!(state.flat_nodes.len(), 3);

        state.tree_cursor = 0;
        state.toggle_expand();
        assert!(!state.expanded_dirs.contains("src"));
        assert_eq!(state.flat_nodes.len(), 2);
    }

    #[test]
    fn test_switch_server_clears_local_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state
            .local_cache
            .insert("a.txt".to_string(), "old content".to_string());
        state
            .remote_cache
            .insert("a.txt".to_string(), "remote content".to_string());
        state.error_paths.insert("a.txt".to_string());

        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);

        assert!(state.local_cache.is_empty());
        assert!(state.remote_cache.is_empty());
        assert!(state.error_paths.is_empty());
    }

    #[test]
    fn test_switch_server_clears_scan_cache_and_filter_mode() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.scan_local_tree = Some(vec![FileNode::new_file("a.txt")]);
        state.scan_remote_tree = Some(vec![FileNode::new_file("a.txt")]);
        state.diff_filter_mode = true;

        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);

        assert!(state.scan_local_tree.is_none());
        assert!(state.scan_remote_tree.is_none());
        assert!(!state.diff_filter_mode);
    }

    #[test]
    fn test_switch_server_clears_undo_stack() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.undo_stack.push_back(CacheSnapshot {
            local_content: "old".to_string(),
            remote_content: "old-remote".to_string(),
            diff: None,
        });

        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);

        assert!(state.undo_stack.is_empty());
        assert!(!state.has_unsaved_changes());
    }

    #[test]
    fn test_clear_scan_cache_disables_filter_mode() {
        let local_nodes = vec![FileNode::new_file("a.txt"), FileNode::new_file("b.txt")];
        let remote_nodes = vec![FileNode::new_file("a.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes.clone()),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.scan_local_tree = Some(local_nodes);
        state.scan_remote_tree = Some(vec![FileNode::new_file("a.txt")]);
        state.diff_filter_mode = true;
        state.rebuild_flat_nodes();

        let nodes_in_filter = state.flat_nodes.len();

        state.clear_scan_cache();

        assert!(!state.diff_filter_mode);
        assert!(state.flat_nodes.len() >= nodes_in_filter);
    }

    #[test]
    fn test_compute_scan_badge_without_scan_cache_returns_unchecked() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.scan_local_tree = None;
        state.scan_remote_tree = None;

        let badge = state.compute_scan_badge("a.txt", false);
        assert_eq!(badge, Badge::Unchecked);
    }

    #[test]
    fn test_compute_scan_badge_prefers_content_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("a.txt".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("a.txt".to_string(), "same".to_string());

        let mut local_node = FileNode::new_file("a.txt");
        local_node.size = Some(100);
        let mut remote_node = FileNode::new_file("a.txt");
        remote_node.size = Some(200);
        state.scan_local_tree = Some(vec![local_node]);
        state.scan_remote_tree = Some(vec![remote_node]);

        let badge = state.compute_scan_badge("a.txt", false);
        assert_eq!(badge, Badge::Equal);
    }

    #[test]
    fn test_switch_server_badge_uses_new_tree() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        let badge_before = state.compute_badge("a.txt", false);
        assert_eq!(badge_before, Badge::Unchecked);

        let new_tree = make_test_tree(vec![FileNode::new_file("b.txt")]);
        state.switch_server("staging".to_string(), new_tree);

        let badge_after = state.compute_badge("a.txt", false);
        assert_eq!(badge_after, Badge::LocalOnly);
    }

    #[test]
    fn test_error_paths_cleared_after_state_reset() {
        let mut state = AppState::new(
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            make_test_tree(vec![FileNode::new_file("a.txt")]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.error_paths.insert("a.txt".to_string());
        assert_eq!(state.compute_badge("a.txt", false), Badge::Error);

        state.clear_cache();
        assert_ne!(state.compute_badge("a.txt", false), Badge::Error);
    }

    #[test]
    fn test_diff_filter_to_server_switch_restores_all_files() {
        let local_nodes = vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ];
        let remote_nodes = vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("same.txt".to_string(), "identical".to_string());
        state
            .remote_cache
            .insert("same.txt".to_string(), "identical".to_string());
        state
            .local_cache
            .insert("changed.txt".to_string(), "local ver".to_string());
        state
            .remote_cache
            .insert("changed.txt".to_string(), "remote ver".to_string());

        state.scan_local_tree = Some(vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ]);
        state.scan_remote_tree = Some(vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ]);

        state.toggle_diff_filter();
        assert!(state.diff_filter_mode);
        let filtered_count = state.flat_nodes.iter().filter(|n| !n.is_dir).count();

        let new_tree = make_test_tree(vec![
            FileNode::new_file("changed.txt"),
            FileNode::new_file("same.txt"),
        ]);
        state.switch_server("staging".to_string(), new_tree);

        assert!(!state.diff_filter_mode);
        let all_count = state.flat_nodes.iter().filter(|n| !n.is_dir).count();
        assert!(all_count >= filtered_count);
    }

    // ── error_paths とバッジ計算のテスト ──

    #[test]
    fn test_badge_local_only_not_error_when_remote_missing() {
        // ローカルにのみ存在するファイル: error_paths に入っていなければ LocalOnly
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("local_only.rs")],
        )];
        // リモートの src はロード済み（空）→ local_only.rs は確実に存在しない
        let remote_nodes = vec![FileNode::new_dir_with_children("src", vec![])];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        // ローカルキャッシュのみ存在（リモートは読めなかった想定）
        state
            .local_cache
            .insert("src/local_only.rs".to_string(), "content".to_string());
        // error_paths には入れない（片方読めれば OK）

        let badge = state.compute_badge("src/local_only.rs", false);
        assert_eq!(badge, Badge::LocalOnly);
    }

    #[test]
    fn test_badge_remote_only_not_error_when_local_missing() {
        // リモートにのみ存在するファイル
        // ローカルの src はロード済み（空）→ remote_only.rs は確実に存在しない
        let local_nodes = vec![FileNode::new_dir_with_children("src", vec![])];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("remote_only.rs")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .remote_cache
            .insert("src/remote_only.rs".to_string(), "content".to_string());

        let badge = state.compute_badge("src/remote_only.rs", false);
        assert_eq!(badge, Badge::RemoteOnly);
    }

    #[test]
    fn test_badge_error_only_when_both_fail() {
        // error_paths に入っているファイルは Error バッジになる
        let local_nodes = vec![FileNode::new_file("broken.rs")];
        let remote_nodes = vec![FileNode::new_file("broken.rs")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.error_paths.insert("broken.rs".to_string());

        let badge = state.compute_badge("broken.rs", false);
        assert_eq!(badge, Badge::Error);
    }

    #[test]
    fn test_collect_diff_includes_local_only_files() {
        // LocalOnly ファイルがバッチマージダイアログに含まれることを検証
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![
                FileNode::new_file("both.rs"),
                FileNode::new_file("local_only.rs"),
            ],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("both.rs")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.toggle_expand();
        // both.rs: 同一内容
        state
            .local_cache
            .insert("src/both.rs".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("src/both.rs".to_string(), "same".to_string());
        // local_only.rs: ローカルのみキャッシュあり（リモートになし → error_paths にも入れない）
        state
            .local_cache
            .insert("src/local_only.rs".to_string(), "only here".to_string());
        state.rebuild_flat_nodes();

        let (files, _) = state.collect_diff_files_under("src");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "src/local_only.rs");
        assert_eq!(files[0].1, Badge::LocalOnly);
    }

    #[test]
    fn test_collect_diff_excludes_error_badge() {
        // error_paths に入ったファイルは collect_diff_files_under に含まれない
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("broken.rs")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("broken.rs")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state.tree_cursor = 0;
        state.toggle_expand();
        state.error_paths.insert("src/broken.rs".to_string());
        state.rebuild_flat_nodes();

        let (files, _) = state.collect_diff_files_under("src");
        assert!(files.is_empty(), "Error badge files should be excluded");
    }

    // ── ディレクトリバッジのテスト ──

    #[test]
    fn test_dir_badge_equal_when_all_children_equal() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state
            .local_cache
            .insert("src/b.ts".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("src/b.ts".to_string(), "same".to_string());

        let badge = state.compute_badge("src", true);
        assert_eq!(badge, Badge::Equal);
    }

    #[test]
    fn test_dir_badge_modified_when_child_differs() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "new".to_string());

        let badge = state.compute_badge("src", true);
        assert_eq!(badge, Badge::Modified);
    }

    #[test]
    fn test_dir_badge_local_only() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];

        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        let badge = state.compute_badge("src", true);
        assert_eq!(badge, Badge::LocalOnly);
    }

    #[test]
    fn test_dir_badge_unchecked_when_no_cache() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts")],
        )];

        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        let badge = state.compute_badge("src", true);
        assert_eq!(badge, Badge::Unchecked);
    }

    #[test]
    fn test_dir_badge_unchecked_when_partial_cache() {
        // 2ファイル中1つだけキャッシュ済みでEqual → 全数確認できてないので Unchecked
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        // a.ts だけキャッシュ済み（Equal）、b.ts は未確認
        state
            .local_cache
            .insert("src/a.ts".to_string(), "same".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "same".to_string());

        let badge = state.compute_badge("src", true);
        assert_eq!(badge, Badge::Unchecked);
    }

    #[test]
    fn test_dir_badge_modified_even_with_unchecked_siblings() {
        // 1つでも差分があれば、未確認ファイルがあっても Modified
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("a.ts"), FileNode::new_file("b.ts")],
        )];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        // a.ts は Modified、b.ts は未確認
        state
            .local_cache
            .insert("src/a.ts".to_string(), "old".to_string());
        state
            .remote_cache
            .insert("src/a.ts".to_string(), "new".to_string());

        let badge = state.compute_badge("src", true);
        assert_eq!(badge, Badge::Modified);
    }

    // ── selection: Local Only 誤判定テスト ──

    #[test]
    fn test_select_file_remote_cache_not_loaded_shows_not_loaded() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        // ローカルキャッシュのみ（リモートは未ロード）
        state
            .local_cache
            .insert("test.txt".to_string(), "content".to_string());
        state.tree_cursor = 0;
        state.select_file();

        // ツリーにリモートがあるので "local only" ではなく "not loaded"
        assert!(
            state.status_message.contains("not loaded"),
            "Expected 'not loaded' but got: {}",
            state.status_message
        );
        // diff は表示される（空文字列との比較）
        assert!(state.current_diff.is_some());
    }

    #[test]
    fn test_select_file_true_local_only_shows_local_only() {
        let local_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "content".to_string());
        state.tree_cursor = 0;
        state.select_file();

        assert!(
            state.status_message.contains("local only"),
            "Expected 'local only' but got: {}",
            state.status_message
        );
        // diff は表示される
        assert!(state.current_diff.is_some());
    }

    // ── sync_cache_after_merge テスト ──

    #[test]
    fn test_sync_cache_after_merge_local_to_remote() {
        let local_nodes = vec![FileNode::new_file("test.txt")];
        let remote_nodes = vec![FileNode::new_file("test.txt")];

        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("test.txt".to_string(), "content".to_string());
        state.sync_cache_after_merge("test.txt", "content", MergeDirection::LocalToRemote);

        // リモートキャッシュが同期される
        assert_eq!(state.remote_cache.get("test.txt").unwrap(), "content");
        // rebuild_flat_nodes は呼ばれない（バッジは古いまま）
    }

    // ── invalidate_cache_for_paths テスト ──

    #[test]
    fn test_invalidate_cache_for_paths_clears_specified_paths() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("a.rs".to_string(), "old_a".to_string());
        state
            .local_cache
            .insert("b.rs".to_string(), "old_b".to_string());
        state
            .local_cache
            .insert("c.rs".to_string(), "keep_c".to_string());
        state
            .remote_cache
            .insert("a.rs".to_string(), "old_a_remote".to_string());
        state
            .remote_cache
            .insert("b.rs".to_string(), "old_b_remote".to_string());

        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        state.invalidate_cache_for_paths(&paths);

        // 指定パスはクリアされる
        assert!(!state.local_cache.contains_key("a.rs"));
        assert!(!state.local_cache.contains_key("b.rs"));
        assert!(!state.remote_cache.contains_key("a.rs"));
        assert!(!state.remote_cache.contains_key("b.rs"));
        // 指定外のパスは残る
        assert_eq!(state.local_cache.get("c.rs").unwrap(), "keep_c");
    }

    #[test]
    fn test_invalidate_cache_for_paths_empty_is_noop() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );

        state
            .local_cache
            .insert("x.rs".to_string(), "content".to_string());

        state.invalidate_cache_for_paths(&[]);

        assert_eq!(state.local_cache.get("x.rs").unwrap(), "content");
    }

    #[test]
    fn test_symlink_badge_equal_when_same_target() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../README.md")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../README.md")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src/link", false), Badge::Equal);
    }

    #[test]
    fn test_symlink_badge_modified_when_different_target() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../README.md")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("link", "../OTHER.md")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src/link", false), Badge::Modified);
    }

    #[test]
    fn test_symlink_badge_modified_when_mixed_types() {
        // ローカルがシンボリックリンク、リモートが通常ファイル
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_symlink("file", "target")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("file")],
        )];
        let state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        assert_eq!(state.compute_badge("src/file", false), Badge::Modified);
    }

    #[test]
    fn test_binary_cache_badge_equal() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        let info = crate::diff::binary::BinaryInfo {
            size: 100,
            sha256: "abc123".to_string(),
        };
        state
            .local_binary_cache
            .insert("src/logo.png".to_string(), info.clone());
        state
            .remote_binary_cache
            .insert("src/logo.png".to_string(), info);
        assert_eq!(state.compute_badge("src/logo.png", false), Badge::Equal);
    }

    #[test]
    fn test_binary_cache_badge_modified() {
        let local_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let remote_nodes = vec![FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("logo.png")],
        )];
        let mut state = AppState::new(
            make_test_tree(local_nodes),
            make_test_tree(remote_nodes),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state.local_binary_cache.insert(
            "src/logo.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 100,
                sha256: "abc".to_string(),
            },
        );
        state.remote_binary_cache.insert(
            "src/logo.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 200,
                sha256: "def".to_string(),
            },
        );
        assert_eq!(state.compute_badge("src/logo.png", false), Badge::Modified);
    }

    #[test]
    fn test_clear_cache_also_clears_binary_cache() {
        let mut state = AppState::new(
            make_test_tree(vec![]),
            make_test_tree(vec![]),
            "develop".to_string(),
            crate::theme::DEFAULT_THEME,
        );
        state.local_binary_cache.insert(
            "x.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 1,
                sha256: "a".to_string(),
            },
        );
        state.remote_binary_cache.insert(
            "x.png".to_string(),
            crate::diff::binary::BinaryInfo {
                size: 1,
                sha256: "a".to_string(),
            },
        );
        state.clear_cache();
        assert!(state.local_binary_cache.is_empty());
        assert!(state.remote_binary_cache.is_empty());
    }
}
