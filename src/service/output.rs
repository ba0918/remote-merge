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

/// status テキスト出力のヘッダ行を生成する
fn format_status_header(left: &str, right: &str, ref_: Option<&SourceInfo>) -> String {
    match ref_ {
        Some(ref_info) => format!(
            "Comparing: {} \u{2194} {} (ref: {})",
            left, right, ref_info.label
        ),
        None => format!("Comparing: {} \u{2194} {}", left, right),
    }
}

/// ref バッジ文字列を表示用テキストに変換する
fn ref_badge_display(badge: &str) -> &'static str {
    match badge {
        "differs" => " [ref≠]",
        "exists_only_in_ref" => " [ref+]",
        "missing_in_ref" => " [ref-]",
        _ => "", // all_equal — don't display
    }
}

/// StatusOutput をテキストフォーマットする（git status 風）
pub fn format_status_text(output: &StatusOutput, summary_only: bool) -> String {
    let mut lines = Vec::new();

    // ヘッダ行: 比較対象を表示
    let header = format_status_header(
        &output.left.label,
        &output.right.label,
        output.ref_.as_ref(),
    );
    lines.push(header);
    lines.push(String::new());

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
                let ref_badge_str = file
                    .ref_badge
                    .as_ref()
                    .map(|b| ref_badge_display(b))
                    .unwrap_or("");
                lines.push(format!(
                    "{}{}{}{}{}",
                    prefix, file.path, sensitive_mark, hunk_info, ref_badge_str
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

    if let (Some(rd), Some(ro), Some(rm)) = (
        output.summary.ref_differs,
        output.summary.ref_only,
        output.summary.ref_missing,
    ) {
        lines.push(format!(
            "  Ref: {} differs, {} ref-only, {} ref-missing",
            rd, ro, rm
        ));
    }

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

    if let (Some(ref_info), Some(ref_hunks)) = (&output.ref_, &output.ref_hunks) {
        if !ref_hunks.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "--- ref:{}:{} (reference diff vs left)",
                ref_info.label, output.path
            ));
            for hunk in ref_hunks {
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
        }
    }

    lines.join("\n")
}

/// 複数ファイル diff のテキスト出力をフォーマットする
pub fn format_multi_diff_text(output: &MultiDiffOutput) -> String {
    let mut result = String::new();
    for (i, diff) in output.files.iter().enumerate() {
        if i > 0 {
            result.push('\n');
        }
        result.push_str(&format_diff_text(diff));
    }
    if output.truncated {
        if let Some(total) = output.total_files {
            result.push_str(&format!(
                "\n... and {} more files (truncated, use --max-files 0 for all)\n",
                total - output.files.len()
            ));
        }
    }
    // Summary line
    result.push_str(&format!(
        "\n{} file(s) with changes out of {} total\n",
        output.summary.files_with_changes, output.summary.total_files
    ));
    result
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
        let ref_badge_str = result
            .ref_badge
            .as_ref()
            .map(|b| ref_badge_display(b))
            .unwrap_or("");
        let prefix = if result.status == "would merge" {
            "Would merge"
        } else {
            "Merged"
        };
        lines.push(format!(
            "{}: {}{}{}",
            prefix, result.path, backup_info, ref_badge_str
        ));
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
            ref_: None,
            files: Some(vec![
                FileStatus {
                    path: "src/config.ts".into(),
                    status: FileStatusKind::Modified,
                    sensitive: false,
                    hunks: None,
                    ref_badge: None,
                },
                FileStatus {
                    path: "src/new.ts".into(),
                    status: FileStatusKind::LeftOnly,
                    sensitive: false,
                    hunks: None,
                    ref_badge: None,
                },
                FileStatus {
                    path: ".env".into(),
                    status: FileStatusKind::Modified,
                    sensitive: true,
                    hunks: None,
                    ref_badge: None,
                },
            ]),
            summary: StatusSummary {
                modified: 2,
                left_only: 1,
                right_only: 0,
                equal: 0,
                ref_differs: None,
                ref_only: None,
                ref_missing: None,
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
            ref_: None,
            files: Some(vec![FileStatus {
                path: "a.rs".into(),
                status: FileStatusKind::Modified,
                sensitive: false,
                hunks: Some(3),
                ref_badge: None,
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
            ref_: None,
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
            ref_hunks: None,
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
            ref_: None,
            sensitive: false,
            truncated: true,
            hunks: vec![],
            ref_hunks: None,
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
                ref_badge: None,
            }],
            skipped: vec![MergeSkipped {
                path: ".env".into(),
                reason: "sensitive file".into(),
            }],
            failed: vec![],
            ref_: None,
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

    #[test]
    fn test_format_status_text_with_ref_badges() {
        let output = StatusOutput {
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
            files: Some(vec![
                FileStatus {
                    path: "a.rs".into(),
                    status: FileStatusKind::Modified,
                    sensitive: false,
                    hunks: Some(2),
                    ref_badge: Some("differs".into()),
                },
                FileStatus {
                    path: "b.rs".into(),
                    status: FileStatusKind::Equal,
                    sensitive: false,
                    hunks: None,
                    ref_badge: Some("missing_in_ref".into()),
                },
                FileStatus {
                    path: "c.rs".into(),
                    status: FileStatusKind::Modified,
                    sensitive: false,
                    hunks: None,
                    ref_badge: Some("all_equal".into()),
                },
            ]),
            summary: StatusSummary {
                modified: 2,
                left_only: 0,
                right_only: 0,
                equal: 1,
                ref_differs: Some(1),
                ref_only: Some(0),
                ref_missing: Some(1),
            },
        };
        let text = format_status_text(&output, false);
        assert!(text.contains("M a.rs (2 hunks) [ref≠]"));
        assert!(text.contains("= b.rs [ref-]"));
        // all_equal badge should NOT be displayed
        assert!(!text.contains("c.rs [ref"));
        assert!(text.contains("Ref: 1 differs, 0 ref-only, 1 ref-missing"));
    }

    #[test]
    fn test_format_status_text_no_ref_backward_compat() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "dev".into(),
                root: "/r".into(),
            },
            ref_: None,
            files: Some(vec![FileStatus {
                path: "a.rs".into(),
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
        let text = format_status_text(&output, false);
        assert!(!text.contains("Ref:"));
        assert!(!text.contains("[ref"));
    }

    #[test]
    fn test_format_diff_text_with_ref_hunks() {
        let output = DiffOutput {
            path: "config.ts".into(),
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
            truncated: false,
            hunks: vec![],
            ref_hunks: Some(vec![DiffHunk {
                index: 0,
                left_start: 1,
                right_start: 1,
                lines: vec![
                    DiffLine {
                        line_type: DiffLineType::Removed,
                        content: "old ref".into(),
                    },
                    DiffLine {
                        line_type: DiffLineType::Added,
                        content: "new ref".into(),
                    },
                ],
            }]),
        };
        let text = format_diff_text(&output);
        assert!(text.contains("--- ref:staging:config.ts (reference diff vs left)"));
        assert!(text.contains("-old ref"));
        assert!(text.contains("+new ref"));
    }

    #[test]
    fn test_format_diff_text_no_ref_backward_compat() {
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
            truncated: false,
            hunks: vec![],
            ref_hunks: None,
        };
        let text = format_diff_text(&output);
        assert!(!text.contains("ref"));
    }

    #[test]
    fn test_format_merge_text_dry_run_prefix() {
        let output = MergeOutput {
            merged: vec![MergeFileResult {
                path: "a.rs".into(),
                status: "would merge".into(),
                backup: None,
                ref_badge: None,
            }],
            skipped: vec![],
            failed: vec![],
            ref_: None,
        };
        let text = format_merge_text(&output);
        assert!(text.contains("Would merge: a.rs"));
        assert!(!text.contains("Merged:"));
    }

    #[test]
    fn test_format_merge_text_dry_run_with_ref_badge() {
        let output = MergeOutput {
            merged: vec![MergeFileResult {
                path: "a.rs".into(),
                status: "would merge".into(),
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
        let text = format_merge_text(&output);
        assert!(text.contains("Would merge: a.rs [ref≠]"));
    }

    #[test]
    fn test_format_merge_text_with_ref_badge() {
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
        let text = format_merge_text(&output);
        assert!(text.contains("Merged: a.rs [ref≠]"));
    }

    // ── multi diff text tests ──

    fn sample_diff(path: &str) -> DiffOutput {
        DiffOutput {
            path: path.into(),
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
            truncated: false,
            hunks: vec![DiffHunk {
                index: 0,
                left_start: 1,
                right_start: 1,
                lines: vec![
                    DiffLine {
                        line_type: DiffLineType::Removed,
                        content: "old".into(),
                    },
                    DiffLine {
                        line_type: DiffLineType::Added,
                        content: "new".into(),
                    },
                ],
            }],
            ref_hunks: None,
        }
    }

    #[test]
    fn test_format_multi_diff_text_single_file() {
        let output = MultiDiffOutput {
            files: vec![sample_diff("a.rs")],
            summary: MultiDiffSummary {
                total_files: 1,
                files_with_changes: 1,
            },
            truncated: false,
            total_files: None,
        };
        let text = format_multi_diff_text(&output);
        assert!(text.contains("--- a/a.rs"));
        assert!(text.contains("+++ b/a.rs"));
        assert!(text.contains("-old"));
        assert!(text.contains("+new"));
        assert!(text.contains("1 file(s) with changes out of 1 total"));
    }

    #[test]
    fn test_format_multi_diff_text_multiple_files() {
        let output = MultiDiffOutput {
            files: vec![sample_diff("a.rs"), sample_diff("b.rs")],
            summary: MultiDiffSummary {
                total_files: 5,
                files_with_changes: 2,
            },
            truncated: false,
            total_files: None,
        };
        let text = format_multi_diff_text(&output);
        assert!(text.contains("--- a/a.rs"));
        assert!(text.contains("--- a/b.rs"));
        assert!(text.contains("2 file(s) with changes out of 5 total"));
    }

    #[test]
    fn test_format_multi_diff_text_truncated() {
        let output = MultiDiffOutput {
            files: vec![sample_diff("a.rs")],
            summary: MultiDiffSummary {
                total_files: 10,
                files_with_changes: 1,
            },
            truncated: true,
            total_files: Some(10),
        };
        let text = format_multi_diff_text(&output);
        assert!(text.contains("... and 9 more files (truncated, use --max-files 0 for all)"));
    }

    // ── status header tests ──

    #[test]
    fn test_status_header_without_ref() {
        let text = format_status_text(&sample_status(), false);
        assert!(text.starts_with("Comparing: local \u{2194} develop"));
        assert!(!text.contains("(ref:"));
    }

    #[test]
    fn test_status_header_with_ref() {
        let output = StatusOutput {
            left: SourceInfo {
                label: "local".into(),
                root: ".".into(),
            },
            right: SourceInfo {
                label: "staging".into(),
                root: "/s".into(),
            },
            ref_: Some(SourceInfo {
                label: "develop".into(),
                root: "/d".into(),
            }),
            files: Some(vec![]),
            summary: StatusSummary::default(),
        };
        let text = format_status_text(&output, false);
        assert!(text.starts_with("Comparing: local \u{2194} staging (ref: develop)"));
    }

    #[test]
    fn test_status_header_appears_in_summary_only_mode() {
        let text = format_status_text(&sample_status(), true);
        assert!(text.contains("Comparing: local \u{2194} develop"));
        assert!(text.contains("Summary:"));
        // ファイル一覧は出力されない
        assert!(!text.contains("src/config.ts"));
    }

    #[test]
    fn test_status_json_not_affected_by_header() {
        let output = sample_status();
        let json = format_json(&output).unwrap();
        // JSON にはヘッダ行が含まれない
        assert!(!json.contains("Comparing:"));
        // JSON の構造は正しい
        assert!(json.contains("\"label\""));
        assert!(json.contains("\"local\""));
    }
}
