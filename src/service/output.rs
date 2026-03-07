//! テキスト/JSON フォーマッター。
//!
//! Service層の出力型を人間向けテキストまたはJSON文字列に変換する。

use super::types::*;

/// 出力フォーマット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    /// 文字列からパースする
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        match s {
            "text" | "diff" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => anyhow::bail!("Unknown format: '{}' (expected text, json, or diff)", other),
        }
    }
}

/// StatusOutput をテキストフォーマットする（git status 風）
pub fn format_status_text(output: &StatusOutput, summary_only: bool) -> String {
    let mut lines = Vec::new();

    if !summary_only {
        if let Some(files) = &output.files {
            for file in files {
                let prefix = match file.status {
                    FileStatusKind::Modified => "M ",
                    FileStatusKind::LeftOnly => "+ ",
                    FileStatusKind::RightOnly => "- ",
                    FileStatusKind::Equal => "= ",
                };
                let sensitive_mark = if file.sensitive { " [SENSITIVE]" } else { "" };
                let hunk_info = file
                    .hunks
                    .map(|n| format!(" ({} hunks)", n))
                    .unwrap_or_default();
                lines.push(format!(
                    "{}{}{}{}",
                    prefix, file.path, sensitive_mark, hunk_info
                ));
            }
            lines.push(String::new());
        }
    }

    lines.push(format!(
        "Summary: {} modified, {} left only, {} right only, {} equal",
        output.summary.modified,
        output.summary.left_only,
        output.summary.right_only,
        output.summary.equal,
    ));

    lines.join("\n")
}

/// DiffOutput をテキストフォーマットする（unified diff 風）
pub fn format_diff_text(output: &DiffOutput) -> String {
    let mut lines = Vec::new();

    lines.push(format!("--- a/{} ({})", output.path, output.left.label));
    lines.push(format!("+++ b/{} ({})", output.path, output.right.label));

    for hunk in &output.hunks {
        lines.push(format!(
            "@@ -{},{} +{},{} @@",
            hunk.left_start,
            hunk.lines
                .iter()
                .filter(|l| l.line_type != DiffLineType::Added)
                .count(),
            hunk.right_start,
            hunk.lines
                .iter()
                .filter(|l| l.line_type != DiffLineType::Removed)
                .count(),
        ));
        for line in &hunk.lines {
            let prefix = match line.line_type {
                DiffLineType::Context => " ",
                DiffLineType::Added => "+",
                DiffLineType::Removed => "-",
            };
            lines.push(format!("{}{}", prefix, line.content));
        }
    }

    if output.truncated {
        lines.push("... (output truncated)".into());
    }

    lines.join("\n")
}

/// MergeOutput をテキストフォーマットする
pub fn format_merge_text(output: &MergeOutput) -> String {
    let mut lines = Vec::new();

    for result in &output.merged {
        let backup_info = result
            .backup
            .as_ref()
            .map(|b| format!(" (backup: {})", b))
            .unwrap_or_default();
        lines.push(format!("Merged: {}{}", result.path, backup_info));
    }

    for skip in &output.skipped {
        lines.push(format!("Skipped: {} ({})", skip.path, skip.reason));
    }

    for fail in &output.failed {
        lines.push(format!("Failed: {} ({})", fail.path, fail.error));
    }

    lines.join("\n")
}

/// 任意の Serialize 可能な型を JSON 文字列にする
pub fn format_json<T: serde::Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_status() -> StatusOutput {
        StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: "/home/user/app".into(),
            },
            right: SourceInfo {
                label: "develop".into(),
                root: "dev:/var/www".into(),
            },
            files: Some(vec![
                FileStatus {
                    path: "src/config.ts".into(),
                    status: FileStatusKind::Modified,
                    sensitive: false,
                    hunks: None,
                },
                FileStatus {
                    path: "src/new.ts".into(),
                    status: FileStatusKind::LeftOnly,
                    sensitive: false,
                    hunks: None,
                },
                FileStatus {
                    path: ".env".into(),
                    status: FileStatusKind::Modified,
                    sensitive: true,
                    hunks: None,
                },
            ]),
            summary: StatusSummary {
                modified: 2,
                left_only: 1,
                right_only: 0,
                equal: 0,
            },
        }
    }

    #[test]
    fn test_format_status_text() {
        let text = format_status_text(&sample_status(), false);
        assert!(text.contains("M src/config.ts"));
        assert!(text.contains("+ src/new.ts"));
        assert!(text.contains("M .env [SENSITIVE]"));
        assert!(text.contains("Summary: 2 modified, 1 left only"));
    }

    #[test]
    fn test_format_status_text_summary_only() {
        let text = format_status_text(&sample_status(), true);
        assert!(!text.contains("src/config.ts"));
        assert!(text.contains("Summary:"));
    }

    #[test]
    fn test_format_status_text_with_hunks() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "l".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "r".into(),
                root: "/r".into(),
            },
            files: Some(vec![FileStatus {
                path: "a.rs".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: Some(3),
            }]),
            summary: StatusSummary {
                modified: 1,
                ..Default::default()
            },
        };
        let text = format_status_text(&output, false);
        assert!(text.contains("(3 hunks)"));
    }

    #[test]
    fn test_format_diff_text() {
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
                        content: "  fn hello() {".into(),
                    },
                    DiffLine {
                        line_type: DiffLineType::Removed,
                        content: "  old".into(),
                    },
                    DiffLine {
                        line_type: DiffLineType::Added,
                        content: "  new".into(),
                    },
                ],
            }],
        };
        let text = format_diff_text(&output);
        assert!(text.contains("--- a/src/config.ts (local)"));
        assert!(text.contains("+++ b/src/config.ts (develop)"));
        assert!(text.contains("-  old"));
        assert!(text.contains("+  new"));
        assert!(!text.contains("truncated"));
    }

    #[test]
    fn test_format_diff_text_truncated() {
        let output = DiffOutput {
            path: "big.rs".into(),
            left: SourceInfo {
                label: "l".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "r".into(),
                root: "/r".into(),
            },
            sensitive: false,
            truncated: true,
            hunks: vec![],
        };
        let text = format_diff_text(&output);
        assert!(text.contains("truncated"));
    }

    #[test]
    fn test_format_merge_text() {
        let output = MergeOutput {
            merged: vec![MergeFileResult {
                path: "a.rs".into(),
                status: "ok".into(),
                backup: Some("a.rs.bak".into()),
            }],
            skipped: vec![MergeSkipped {
                path: ".env".into(),
                reason: "sensitive file".into(),
            }],
            failed: vec![],
        };
        let text = format_merge_text(&output);
        assert!(text.contains("Merged: a.rs (backup: a.rs.bak)"));
        assert!(text.contains("Skipped: .env (sensitive file)"));
    }

    #[test]
    fn test_format_json() {
        let output = sample_status();
        let json = format_json(&output).unwrap();
        assert!(json.contains("\"modified\""));
        assert!(json.contains("\"local\""));
    }

    #[test]
    fn test_output_format_parse() {
        assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::parse("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::parse("diff").unwrap(), OutputFormat::Text);
        assert!(OutputFormat::parse("yaml").is_err());
    }
}
