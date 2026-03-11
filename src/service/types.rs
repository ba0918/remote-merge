//! Service層の入出力型定義。
//!
//! CLI/MCP 双方で共通に使える構造体。
//! Serialize/Deserialize 対応で JSON 出力が可能。

use serde::{Deserialize, Serialize};

// ── status ──

/// Agent 接続状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Connected,
    Fallback,
}

/// status コマンドの出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusOutput {
    pub left: SourceInfo,
    pub right: SourceInfo,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<SourceInfo>,
    /// Agent 接続状態
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileStatus>>,
    pub summary: StatusSummary,
}

/// ソース情報（左右のどちらか）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub label: String,
    pub root: String,
}

/// ファイル差分ステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    pub path: String,
    pub status: FileStatusKind,
    pub sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hunks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_badge: Option<String>,
}

/// ファイル差分の種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatusKind {
    Modified,
    LeftOnly,
    RightOnly,
    Equal,
}

/// status の集計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusSummary {
    pub modified: usize,
    pub left_only: usize,
    pub right_only: usize,
    pub equal: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_differs: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_only: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_missing: Option<usize>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

// ── diff ──

/// diff コマンドの出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffOutput {
    pub path: String,
    pub left: SourceInfo,
    pub right: SourceInfo,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<SourceInfo>,
    pub sensitive: bool,
    /// バイナリファイルの場合 true（hunks は空になる）
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// シンボリックリンクの場合 true（hunks は空になる）
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub symlink: bool,
    pub truncated: bool,
    pub hunks: Vec<DiffHunk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_hunks: Option<Vec<DiffHunk>>,
    /// バイナリファイルの左側 SHA-256 ハッシュ
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_hash: Option<String>,
    /// バイナリファイルの右側 SHA-256 ハッシュ
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_hash: Option<String>,
    /// 補足情報（sensitive マスク時・type mismatch 時等）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// コンフリクト数（3way diff 時のみ）
    #[serde(skip_serializing_if = "is_zero")]
    pub conflict_count: usize,
    /// コンフリクト領域（3way diff 時のみ。空なら省略）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflict_regions: Vec<crate::diff::conflict::ConflictRegion>,
}

/// diff のハンク
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub index: usize,
    pub left_start: usize,
    pub right_start: usize,
    pub lines: Vec<DiffLine>,
}

/// diff の行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    #[serde(rename = "type")]
    pub line_type: DiffLineType,
    pub content: String,
}

/// diff 行の種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineType {
    Context,
    Added,
    Removed,
}

// ── multi diff ──

/// 複数ファイルの diff 出力（ディレクトリ・複数パス指定時の統一型）
#[derive(Debug, Clone, Serialize)]
pub struct MultiDiffOutput {
    pub files: Vec<DiffOutput>,
    pub summary: MultiDiffSummary,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_files: Option<usize>,
}

/// MultiDiffOutput のサマリー
#[derive(Debug, Clone, Serialize)]
pub struct MultiDiffSummary {
    pub total_files: usize,
    pub files_with_changes: usize,
}

// ── merge ──

/// merge コマンドの出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeOutput {
    pub merged: Vec<MergeFileResult>,
    pub skipped: Vec<MergeSkipped>,
    pub failed: Vec<MergeFailure>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<SourceInfo>,
}

/// マージ成功結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeFileResult {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_badge: Option<String>,
}

/// マージスキップ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSkipped {
    pub path: String,
    pub reason: String,
}

/// マージ失敗
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeFailure {
    pub path: String,
    pub error: String,
}

/// merge サービスの実行結果。CLI/TUI 共通。
#[derive(Debug, Clone)]
pub enum MergeOutcome {
    /// マージ成功（0件以上のファイルがマージされた）
    Success(MergeOutput),
    /// 指定パスにマージ対象ファイルがない
    NoFilesToMerge,
    /// remote-to-remote マージがブロックされた（--force で解除可能）
    R2rBlocked { left: String, right: String },
}

// ── rollback ──

/// rollback --list の出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupListOutput {
    pub target: SourceInfo,
    pub sessions: Vec<BackupSession>,
}

/// バックアップセッション（= 1回のマージ操作）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSession {
    pub session_id: String,
    pub files: Vec<BackupEntry>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub expired: bool,
}

/// バックアップエントリ（セッション内の1ファイル）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub path: String,
    pub size: u64,
}

/// rollback 実行結果の出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackOutput {
    pub target: SourceInfo,
    pub session_id: String,
    pub restored: Vec<RollbackFileResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<RollbackSkipped>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<RollbackFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackFileResult {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_rollback_backup: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackSkipped {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackFailure {
    pub path: String,
    pub error: String,
}

// ── exit codes ──

/// CLI exit code 定義
pub mod exit_code {
    /// 成功（差分なし / マージ成功）
    pub const SUCCESS: i32 = 0;
    /// 成功（差分あり）
    pub const DIFF_FOUND: i32 = 1;
    /// エラー（接続失敗・設定不備等）
    pub const ERROR: i32 = 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_output_serialize() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: "/home/user/app".into(),
            },
            right: SourceInfo {
                label: "develop".into(),
                root: "dev.example.com:/var/www/app".into(),
            },
            ref_: None,
            agent: None,
            files: Some(vec![FileStatus {
                path: "src/config.ts".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: None,
                ref_badge: None,
            }]),
            summary: StatusSummary {
                modified: 1,
                left_only: 0,
                right_only: 0,
                equal: 0,
                ref_differs: None,
                ref_only: None,
                ref_missing: None,
            },
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"modified\""));
        assert!(json.contains("\"local\""));
        // hunks が None のときは出力されない
        assert!(!json.contains("\"hunks\""));
    }

    #[test]
    fn test_status_output_with_hunks() {
        let file = FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: Some(3),
            ref_badge: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("\"hunks\":3"));
    }

    #[test]
    fn test_status_summary_serialize() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
            ref_: None,
            agent: None,
            files: None,
            summary: StatusSummary {
                modified: 2,
                left_only: 1,
                right_only: 0,
                equal: 10,
                ref_differs: None,
                ref_only: None,
                ref_missing: None,
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        // --summary 時は files が None → 出力されない
        assert!(!json.contains("\"files\""));
        assert!(json.contains("\"modified\":2"));
    }

    #[test]
    fn test_diff_output_serialize() {
        let output = DiffOutput {
            path: "src/config.ts".into(),
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
            ref_: None,
            sensitive: false,
            binary: false,
            symlink: false,
            truncated: false,
            hunks: vec![DiffHunk {
                index: 0,
                left_start: 10,
                right_start: 10,
                lines: vec![
                    DiffLine {
                        line_type: DiffLineType::Context,
                        content: "  function hello() {".into(),
                    },
                    DiffLine {
                        line_type: DiffLineType::Removed,
                        content: "  old line".into(),
                    },
                    DiffLine {
                        line_type: DiffLineType::Added,
                        content: "  new line".into(),
                    },
                ],
            }],
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"context\""));
        assert!(json.contains("\"removed\""));
        assert!(json.contains("\"added\""));
        // conflict_count=0 は JSON に含まれない
        assert!(!json.contains("\"conflict_count\""));
        assert!(!json.contains("\"conflict_regions\""));
    }

    #[test]
    fn test_merge_output_serialize() {
        let output = MergeOutput {
            merged: vec![MergeFileResult {
                path: "src/config.ts".into(),
                status: "ok".into(),
                backup: Some("src/config.ts.20260307.bak".into()),
                ref_badge: None,
            }],
            skipped: vec![MergeSkipped {
                path: ".env".into(),
                reason: "sensitive file".into(),
            }],
            failed: vec![],
            ref_: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"merged\""));
        assert!(json.contains("\"skipped\""));
        assert!(json.contains("\"sensitive file\""));
    }

    #[test]
    fn test_file_status_kind_serializes_snake_case() {
        let kind = FileStatusKind::LeftOnly;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"left_only\"");
    }

    #[test]
    fn test_diff_line_type_serializes_snake_case() {
        let t = DiffLineType::Context;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"context\"");
    }

    #[test]
    fn test_roundtrip_status_output() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "dev".into(),
                root: "/var/www".into(),
            },
            ref_: None,
            agent: None,
            files: Some(vec![]),
            summary: StatusSummary::default(),
        };
        let json = serde_json::to_string(&output).unwrap();
        let parsed: StatusOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.left.label, "local");
        assert_eq!(parsed.right.label, "dev");
    }

    #[test]
    fn test_agent_status_serialize() {
        let connected = AgentStatus::Connected;
        let json = serde_json::to_string(&connected).unwrap();
        assert_eq!(json, "\"connected\"");

        let fallback = AgentStatus::Fallback;
        let json = serde_json::to_string(&fallback).unwrap();
        assert_eq!(json, "\"fallback\"");
    }

    #[test]
    fn test_agent_status_deserialize() {
        let connected: AgentStatus = serde_json::from_str("\"connected\"").unwrap();
        assert_eq!(connected, AgentStatus::Connected);

        let fallback: AgentStatus = serde_json::from_str("\"fallback\"").unwrap();
        assert_eq!(fallback, AgentStatus::Fallback);
    }

    // ── ref field serialization ──

    #[test]
    fn test_status_output_with_ref_serialize() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
            ref_: Some(SourceInfo {
                label: "staging".into(),
                root: "stg:/var/www".into(),
            }),
            agent: None,
            files: None,
            summary: StatusSummary {
                modified: 1,
                left_only: 0,
                right_only: 0,
                equal: 0,
                ref_differs: Some(1),
                ref_only: None,
                ref_missing: None,
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"ref\""));
        assert!(json.contains("\"staging\""));
        assert!(json.contains("\"ref_differs\":1"));
        assert!(!json.contains("\"ref_only\""));
        assert!(!json.contains("\"ref_missing\""));
    }

    #[test]
    fn test_status_output_without_ref_serialize() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
            ref_: None,
            agent: None,
            files: None,
            summary: StatusSummary::default(),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("\"ref\""));
        assert!(!json.contains("ref_differs"));
    }

    #[test]
    fn test_diff_output_with_ref_hunks_serialize() {
        let output = DiffOutput {
            path: "a.rs".into(),
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "dev".into(),
                root: "/r".into(),
            },
            ref_: Some(SourceInfo {
                label: "staging".into(),
                root: "/s".into(),
            }),
            sensitive: false,
            binary: false,
            symlink: false,
            truncated: false,
            hunks: vec![],
            ref_hunks: Some(vec![]),
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"ref\""));
        assert!(json.contains("\"ref_hunks\""));
    }

    #[test]
    fn test_diff_output_without_ref_serialize() {
        let output = DiffOutput {
            path: "a.rs".into(),
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "dev".into(),
                root: "/r".into(),
            },
            ref_: None,
            sensitive: false,
            binary: false,
            symlink: false,
            truncated: false,
            hunks: vec![],
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 0,
            conflict_regions: vec![],
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("\"ref\""));
        assert!(!json.contains("\"ref_hunks\""));
    }

    #[test]
    fn test_merge_output_with_ref_serialize() {
        let output = MergeOutput {
            merged: vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: Some("differs".into()),
            }],
            skipped: vec![],
            failed: vec![],
            ref_: Some(SourceInfo {
                label: "staging".into(),
                root: "/s".into(),
            }),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"ref\""));
        assert!(json.contains("\"ref_badge\""));
        assert!(json.contains("\"differs\""));
    }

    #[test]
    fn test_file_status_ref_badge_serialize() {
        let file = FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: Some(2),
            ref_badge: Some("differs".into()),
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("\"ref_badge\":\"differs\""));
    }

    #[test]
    fn test_file_status_ref_badge_none_omitted() {
        let file = FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
            sensitive: false,
            hunks: None,
            ref_badge: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(!json.contains("ref_badge"));
    }

    // ── multi diff ──

    #[test]
    fn test_multi_diff_output_serialize() {
        let output = MultiDiffOutput {
            files: vec![DiffOutput {
                path: "a.rs".into(),
                left: SourceInfo {
                    label: "local".into(),
                    root: ".".into(),
                },
                right: SourceInfo {
                    label: "dev".into(),
                    root: "/r".into(),
                },
                ref_: None,
                sensitive: false,
                binary: false,
                symlink: false,
                truncated: false,
                hunks: vec![],
                ref_hunks: None,
                left_hash: None,
                right_hash: None,
                note: None,
                conflict_count: 0,
                conflict_regions: vec![],
            }],
            summary: MultiDiffSummary {
                total_files: 5,
                files_with_changes: 1,
            },
            truncated: true,
            total_files: Some(5),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"total_files\":5"));
        assert!(json.contains("\"files_with_changes\":1"));
        assert!(json.contains("\"truncated\":true"));
        assert!(json.contains("\"a.rs\""));
    }

    #[test]
    fn test_multi_diff_output_truncated_false_omitted() {
        let output = MultiDiffOutput {
            files: vec![],
            summary: MultiDiffSummary {
                total_files: 0,
                files_with_changes: 0,
            },
            truncated: false,
            total_files: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("\"truncated\""));
    }

    #[test]
    fn test_multi_diff_output_total_files_none_omitted() {
        let output = MultiDiffOutput {
            files: vec![],
            summary: MultiDiffSummary {
                total_files: 0,
                files_with_changes: 0,
            },
            truncated: false,
            total_files: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        // total_files at the top level should be omitted when None
        // (summary.total_files is always present)
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("total_files").is_none());
    }

    #[test]
    fn test_status_summary_backward_compat_deserialize() {
        // Old JSON without ref fields should deserialize with ref_* = None
        let json = r#"{"modified":1,"left_only":0,"right_only":0,"equal":5}"#;
        let summary: StatusSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.modified, 1);
        assert_eq!(summary.equal, 5);
        assert!(summary.ref_differs.is_none());
        assert!(summary.ref_only.is_none());
        assert!(summary.ref_missing.is_none());
    }

    #[test]
    fn test_roundtrip_status_output_with_ref() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "dev".into(),
                root: "/var/www".into(),
            },
            ref_: Some(SourceInfo {
                label: "staging".into(),
                root: "/s".into(),
            }),
            agent: None,
            files: Some(vec![]),
            summary: StatusSummary {
                modified: 0,
                left_only: 0,
                right_only: 0,
                equal: 0,
                ref_differs: Some(0),
                ref_only: Some(0),
                ref_missing: Some(0),
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        let parsed: StatusOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.ref_.as_ref().unwrap().label, "staging");
        assert_eq!(parsed.summary.ref_differs, Some(0));
    }

    #[test]
    fn test_diff_output_conflict_count_nonzero_serialized() {
        use crate::diff::conflict::ConflictRegion;

        let output = DiffOutput {
            path: "a.rs".into(),
            left: SourceInfo {
                label: "l".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "r".into(),
                root: "/r".into(),
            },
            ref_: None,
            sensitive: false,
            binary: false,
            symlink: false,
            truncated: false,
            hunks: vec![],
            ref_hunks: None,
            left_hash: None,
            right_hash: None,
            note: None,
            conflict_count: 1,
            conflict_regions: vec![ConflictRegion {
                ref_range: 0..1,
                left_lines: vec!["B".into()],
                right_lines: vec!["C".into()],
                left_diff_range: Some(0..1),
                right_diff_range: Some(0..1),
                left_file_lines: Default::default(),
                right_file_lines: Default::default(),
            }],
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"conflict_count\":1"));
        assert!(json.contains("\"conflict_regions\""));
    }

    // ── rollback types ──

    #[test]
    fn test_rollback_output_roundtrip() {
        let output = RollbackOutput {
            target: SourceInfo {
                label: "develop".into(),
                root: "/var/www".into(),
            },
            session_id: "20240115-140000".into(),
            restored: vec![RollbackFileResult {
                path: "src/config.ts".into(),
                pre_rollback_backup: Some("20240120-100000".into()),
            }],
            skipped: vec![RollbackSkipped {
                path: ".env".into(),
                reason: "sensitive".into(),
            }],
            failed: vec![RollbackFailure {
                path: "locked.rs".into(),
                error: "permission denied".into(),
            }],
        };
        let json = serde_json::to_string(&output).unwrap();
        let parsed: RollbackOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "20240115-140000");
        assert_eq!(parsed.restored.len(), 1);
        assert_eq!(parsed.skipped.len(), 1);
        assert_eq!(parsed.failed.len(), 1);
        assert_eq!(
            parsed.restored[0].pre_rollback_backup.as_deref(),
            Some("20240120-100000")
        );
    }

    #[test]
    fn test_backup_list_output_roundtrip() {
        let output = BackupListOutput {
            target: SourceInfo {
                label: "staging".into(),
                root: "/var/www".into(),
            },
            sessions: vec![
                BackupSession {
                    session_id: "20240115-140000".into(),
                    files: vec![BackupEntry {
                        path: "src/app.rs".into(),
                        size: 1024,
                    }],
                    expired: false,
                },
                BackupSession {
                    session_id: "20240101-100000".into(),
                    files: vec![],
                    expired: true,
                },
            ],
        };
        let json = serde_json::to_string(&output).unwrap();
        let parsed: BackupListOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.sessions.len(), 2);
        assert_eq!(parsed.sessions[0].session_id, "20240115-140000");
        assert!(parsed.sessions[1].expired);
    }

    #[test]
    fn test_backup_session_expired_false_skip_serializing() {
        let session = BackupSession {
            session_id: "20240115-140000".into(),
            files: vec![],
            expired: false,
        };
        let json = serde_json::to_string(&session).unwrap();
        // expired=false のとき JSON に含まれない（skip_serializing_if）
        assert!(!json.contains("\"expired\""));
    }

    #[test]
    fn test_rollback_output_empty_skipped_and_failed_omitted() {
        let output = RollbackOutput {
            target: SourceInfo {
                label: "dev".into(),
                root: "/r".into(),
            },
            session_id: "20240115-140000".into(),
            restored: vec![RollbackFileResult {
                path: "a.rs".into(),
                pre_rollback_backup: None,
            }],
            skipped: vec![],
            failed: vec![],
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(!json.contains("\"skipped\""));
        assert!(!json.contains("\"failed\""));
        // pre_rollback_backup が None のときも省略
        assert!(!json.contains("\"pre_rollback_backup\""));
    }
}
