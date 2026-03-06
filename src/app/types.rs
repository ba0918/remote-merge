//! TUI アプリケーションで使用する型定義。

use crate::diff::engine::DiffResult;
use crate::tree::FileNode;

/// undo スタックの最大保持数
pub const MAX_UNDO_STACK: usize = 50;

/// キャッシュのスナップショット（undo 用）
#[derive(Debug, Clone)]
pub struct CacheSnapshot {
    pub local_content: String,
    pub remote_content: String,
    /// 適用時の diff 結果
    pub diff: Option<DiffResult>,
}

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
    /// `[...]` Loading - コンテンツ取得中
    Loading,
    /// `[!]` Error - 取得失敗
    Error,
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
            Badge::Loading => "[..]",
            Badge::Error => "[!]",
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

/// 全走査の状態（変更ファイルフィルター用）
#[derive(Debug, Clone, Default)]
pub enum ScanState {
    /// 未走査
    #[default]
    Idle,
    /// 走査中
    Scanning,
    /// 走査完了（ローカル全ツリー, リモート全ツリー）
    Complete(Vec<FileNode>, Vec<FileNode>),
    /// 部分完了（ローカル, リモート, 理由メッセージ）
    PartialComplete(Vec<FileNode>, Vec<FileNode>, String),
    /// エラー
    Error(String),
}

/// マージ走査の進捗メッセージ（スレッド → メインスレッド）
#[derive(Debug)]
pub enum MergeScanMsg {
    /// 途中経過: 発見ファイル数と処理中パスの更新
    Progress {
        files_found: usize,
        current_path: Option<String>,
    },
    /// コンテンツ読み込みフェーズに遷移（total 確定）
    ContentPhase { total: usize },
    /// 走査完了
    Done(Box<MergeScanResult>),
    /// エラー
    Error(String),
}

/// マージ走査完了時の結果
#[derive(Debug)]
pub struct MergeScanResult {
    /// ローカルファイル内容キャッシュ (パス -> 内容)
    pub local_cache: std::collections::HashMap<String, String>,
    /// リモートファイル内容キャッシュ (パス -> 内容)
    pub remote_cache: std::collections::HashMap<String, String>,
    /// ローカルツリー更新 (パス -> 子ノード)
    pub local_tree_updates: Vec<(String, Vec<FileNode>)>,
    /// リモートツリー更新 (パス -> 子ノード)
    pub remote_tree_updates: Vec<(String, Vec<FileNode>)>,
    /// エラーパス
    pub error_paths: std::collections::HashSet<String>,
}

/// マージ走査の状態
#[derive(Debug, Clone, Default)]
pub enum MergeScanState {
    /// 走査していない
    #[default]
    Idle,
    /// 走査中
    Scanning {
        /// 走査対象ディレクトリパス
        dir_path: String,
        /// マージ方向
        direction: crate::merge::executor::MergeDirection,
    },
}

/// ツリーマージ用の一時ノード
#[derive(Debug, Clone)]
pub struct MergedNode {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub children: Vec<MergedNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_badge_label_all_variants() {
        assert_eq!(Badge::Modified.label(), "[M]");
        assert_eq!(Badge::Equal.label(), "[=]");
        assert_eq!(Badge::LocalOnly.label(), "[+]");
        assert_eq!(Badge::RemoteOnly.label(), "[-]");
        assert_eq!(Badge::Unchecked.label(), "[?]");
        assert_eq!(Badge::Loading.label(), "[..]");
        assert_eq!(Badge::Error.label(), "[!]");
    }

    #[test]
    fn test_badge_equality() {
        assert_eq!(Badge::Modified, Badge::Modified);
        assert_ne!(Badge::Modified, Badge::Equal);
    }

    #[test]
    fn test_max_undo_stack_value() {
        assert_eq!(MAX_UNDO_STACK, 50);
    }

    #[test]
    fn test_focus_variants() {
        assert_eq!(Focus::FileTree, Focus::FileTree);
        assert_ne!(Focus::FileTree, Focus::DiffView);
    }

    #[test]
    fn test_diff_mode_variants() {
        assert_eq!(DiffMode::Unified, DiffMode::Unified);
        assert_ne!(DiffMode::Unified, DiffMode::SideBySide);
    }

    #[test]
    fn test_scan_state_default_is_idle() {
        let state = ScanState::default();
        assert!(matches!(state, ScanState::Idle));
    }

    #[test]
    fn test_merge_scan_state_default_is_idle() {
        let state = MergeScanState::default();
        assert!(matches!(state, MergeScanState::Idle));
    }

    #[test]
    fn test_flat_node_construction() {
        let node = FlatNode {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            depth: 1,
            is_dir: false,
            is_symlink: false,
            expanded: false,
            badge: Badge::Modified,
        };
        assert_eq!(node.path, "src/main.rs");
        assert_eq!(node.badge, Badge::Modified);
        assert!(!node.is_dir);
    }

    #[test]
    fn test_cache_snapshot_clone() {
        let snapshot = CacheSnapshot {
            local_content: "hello".to_string(),
            remote_content: "world".to_string(),
            diff: None,
        };
        let cloned = snapshot.clone();
        assert_eq!(cloned.local_content, "hello");
        assert_eq!(cloned.remote_content, "world");
        assert!(cloned.diff.is_none());
    }

    #[test]
    fn test_merged_node_with_children() {
        let child = MergedNode {
            name: "file.rs".to_string(),
            is_dir: false,
            is_symlink: false,
            children: vec![],
        };
        let parent = MergedNode {
            name: "src".to_string(),
            is_dir: true,
            is_symlink: false,
            children: vec![child],
        };
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].name, "file.rs");
    }
}
