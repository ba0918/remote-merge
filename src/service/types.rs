//! Service層の入出力型定義。
//!
//! CLI/MCP 双方で共通に使える構造体。
//! Serialize/Deserialize 対応で JSON 出力が可能。

use serde::{Deserialize, Serialize};

// ── status ──

/// status コマンドの出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusOutput {
    pub left: SourceInfo,
    pub right: SourceInfo,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<SourceInfo>,
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
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"context\""));
        assert!(json.contains("\"removed\""));
        assert!(json.contains("\"added\""));
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
            files: Some(vec![]),
            summary: StatusSummary::default(),
        };
        let json = serde_json::to_string(&output).unwrap();
        let parsed: StatusOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.left.label, "local");
        assert_eq!(parsed.right.label, "dev");
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
}
