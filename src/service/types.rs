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
}

// ── diff ──

/// diff コマンドの出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffOutput {
    pub path: String,
    pub left: SourceInfo,
    pub right: SourceInfo,
    pub sensitive: bool,
    pub truncated: bool,
    pub hunks: Vec<DiffHunk>,
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

// ── merge ──

/// merge コマンドの出力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeOutput {
    pub merged: Vec<MergeFileResult>,
    pub skipped: Vec<MergeSkipped>,
    pub failed: Vec<MergeFailure>,
}

/// マージ成功結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeFileResult {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
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
            files: Some(vec![FileStatus {
                path: "src/config.ts".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: None,
            }]),
            summary: StatusSummary {
                modified: 1,
                left_only: 0,
                right_only: 0,
                equal: 0,
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
            files: None,
            summary: StatusSummary {
                modified: 2,
                left_only: 1,
                right_only: 0,
                equal: 10,
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
            sensitive: false,
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
            }],
            skipped: vec![MergeSkipped {
                path: ".env".into(),
                reason: "sensitive file".into(),
            }],
            failed: vec![],
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
            files: Some(vec![]),
            summary: StatusSummary::default(),
        };
        let json = serde_json::to_string(&output).unwrap();
        let parsed: StatusOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.left.label, "local");
        assert_eq!(parsed.right.label, "dev");
    }
}
