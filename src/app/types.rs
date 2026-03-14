//! TUI アプリケーションで使用する型定義。

use crate::diff::binary::BinaryInfo;
use crate::tree::FileNode;

/// undo スタックの最大保持数
pub const MAX_UNDO_STACK: usize = 50;

/// キャッシュのスナップショット（undo 用）
///
/// diff は保存せず、復元時に `compute_diff()` で再計算する。
#[derive(Debug, Clone)]
pub struct CacheSnapshot {
    pub local_content: String,
    pub remote_content: String,
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
    /// `[+]` Left Only（左側にのみ存在）
    LeftOnly,
    /// `[-]` Right Only（右側にのみ存在）
    RightOnly,
    /// `[?]` Unchecked - 未比較
    Unchecked,
    /// `[...]` Loading - コンテンツ取得中
    Loading,
    /// `[!]` Error - 取得失敗
    Error,
    /// `[~]` ScanSkipped - スキャン上限超過でスキップ
    ScanSkipped,
}

impl Badge {
    /// バッジの表示文字列
    pub fn label(&self) -> &'static str {
        match self {
            Badge::Modified => "[M]",
            Badge::Equal => "[=]",
            Badge::LeftOnly => "[+]",
            Badge::RightOnly => "[-]",
            Badge::Unchecked => "[?]",
            Badge::Loading => "[..]",
            Badge::Error => "[!]",
            Badge::ScanSkipped => "[~]",
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
    /// reference サーバにのみ存在するノード（グレイ表示用）
    pub ref_only: bool,
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
    /// Agent 操作失敗（SSH フォールバック後に送信）
    AgentFailed { server_name: String },
}

/// マージ走査完了時の結果
#[derive(Debug)]
pub struct MergeScanResult {
    /// ローカルファイル内容キャッシュ (パス -> 内容)
    pub local_cache: std::collections::HashMap<String, String>,
    /// リモートファイル内容キャッシュ (パス -> 内容)
    pub remote_cache: std::collections::HashMap<String, String>,
    /// ローカルバイナリ情報キャッシュ (パス -> BinaryInfo)
    pub local_binary_cache: std::collections::HashMap<String, crate::diff::binary::BinaryInfo>,
    /// リモートバイナリ情報キャッシュ (パス -> BinaryInfo)
    pub remote_binary_cache: std::collections::HashMap<String, crate::diff::binary::BinaryInfo>,
    /// reference サーバのファイル内容キャッシュ (パス -> 内容)
    pub ref_cache: std::collections::HashMap<String, String>,
    /// reference サーバのバイナリ情報キャッシュ (パス -> BinaryInfo)
    pub ref_binary_cache: std::collections::HashMap<String, crate::diff::binary::BinaryInfo>,
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
    /// reference サーバにのみ存在する（left/right 両方に存在しない）
    pub ref_only: bool,
}

/// バッジスキャンの進捗メッセージ（ワーカースレッド → メインスレッド）
#[derive(Debug)]
pub enum BadgeScanMsg {
    /// 1ファイルのスキャン結果（コンテンツ + バイナリ情報）
    FileResult {
        path: String,
        left_content: Option<String>,
        right_content: Option<String>,
        left_binary: Option<BinaryInfo>,
        right_binary: Option<BinaryInfo>,
    },
    /// スキャン完了
    Done { dir_path: String },
    /// エラー（致命的でない、ログのみ）
    Error { path: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_badge_label_all_variants() {
        assert_eq!(Badge::Modified.label(), "[M]");
        assert_eq!(Badge::Equal.label(), "[=]");
        assert_eq!(Badge::LeftOnly.label(), "[+]");
        assert_eq!(Badge::RightOnly.label(), "[-]");
        assert_eq!(Badge::Unchecked.label(), "[?]");
        assert_eq!(Badge::Loading.label(), "[..]");
        assert_eq!(Badge::Error.label(), "[!]");
        assert_eq!(Badge::ScanSkipped.label(), "[~]");
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
            ref_only: false,
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
        };
        let cloned = snapshot.clone();
        assert_eq!(cloned.local_content, "hello");
        assert_eq!(cloned.remote_content, "world");
    }

    #[test]
    fn test_merged_node_with_children() {
        let child = MergedNode {
            name: "file.rs".to_string(),
            is_dir: false,
            is_symlink: false,
            children: vec![],
            ref_only: false,
        };
        let parent = MergedNode {
            name: "src".to_string(),
            is_dir: true,
            is_symlink: false,
            children: vec![child],
            ref_only: false,
        };
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].name, "file.rs");
    }

    #[test]
    fn test_badge_scan_msg_file_result() {
        let msg = BadgeScanMsg::FileResult {
            path: "src/main.rs".to_string(),
            left_content: Some("hello".to_string()),
            right_content: Some("world".to_string()),
            left_binary: None,
            right_binary: None,
        };
        match msg {
            BadgeScanMsg::FileResult {
                path,
                left_content,
                right_content,
                ..
            } => {
                assert_eq!(path, "src/main.rs");
                assert_eq!(left_content.unwrap(), "hello");
                assert_eq!(right_content.unwrap(), "world");
            }
            _ => panic!("Expected FileResult variant"),
        }
    }

    #[test]
    fn test_badge_scan_msg_done() {
        let msg = BadgeScanMsg::Done {
            dir_path: "src".to_string(),
        };
        match msg {
            BadgeScanMsg::Done { dir_path } => assert_eq!(dir_path, "src"),
            _ => panic!("Expected Done variant"),
        }
    }

    #[test]
    fn test_badge_scan_msg_error() {
        let msg = BadgeScanMsg::Error {
            path: "src/main.rs".to_string(),
            message: "read failed".to_string(),
        };
        match msg {
            BadgeScanMsg::Error { path, message } => {
                assert_eq!(path, "src/main.rs");
                assert_eq!(message, "read failed");
            }
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_badge_scan_msg_file_result_with_binary() {
        let info = BinaryInfo::from_bytes(&[0u8; 32]);
        let msg = BadgeScanMsg::FileResult {
            path: "img.png".to_string(),
            left_content: None,
            right_content: None,
            left_binary: Some(info.clone()),
            right_binary: Some(info),
        };
        match msg {
            BadgeScanMsg::FileResult {
                left_binary,
                right_binary,
                ..
            } => {
                assert!(left_binary.is_some());
                assert!(right_binary.is_some());
            }
            _ => panic!("Expected FileResult variant"),
        }
    }

    #[test]
    fn test_badge_scan_msg_file_result_left_only() {
        let msg = BadgeScanMsg::FileResult {
            path: "left_only.rs".to_string(),
            left_content: Some("content".to_string()),
            right_content: None,
            left_binary: None,
            right_binary: None,
        };
        match msg {
            BadgeScanMsg::FileResult {
                left_content,
                right_content,
                ..
            } => {
                assert!(left_content.is_some());
                assert!(right_content.is_none());
            }
            _ => panic!("Expected FileResult variant"),
        }
    }

    #[test]
    fn test_badge_scan_msg_file_result_right_only() {
        let msg = BadgeScanMsg::FileResult {
            path: "right_only.rs".to_string(),
            left_content: None,
            right_content: Some("content".to_string()),
            left_binary: None,
            right_binary: None,
        };
        match msg {
            BadgeScanMsg::FileResult {
                left_content,
                right_content,
                ..
            } => {
                assert!(left_content.is_none());
                assert!(right_content.is_some());
            }
            _ => panic!("Expected FileResult variant"),
        }
    }

    #[test]
    fn test_merge_scan_msg_agent_failed_variant() {
        let msg = MergeScanMsg::AgentFailed {
            server_name: "develop".to_string(),
        };
        match msg {
            MergeScanMsg::AgentFailed { server_name } => {
                assert_eq!(server_name, "develop");
            }
            _ => panic!("Expected AgentFailed variant"),
        }
    }
}
