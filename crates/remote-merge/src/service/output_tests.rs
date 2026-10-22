//! text と JSON の書き出しの単体テスト
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
        agent: None,
        files: Some(vec![
            FileStatus {
                path: "src/config.ts".into(),
                status: FileStatusKind::Modified,
                ref_badge: None,
            },
            FileStatus {
                path: "src/new.ts".into(),
                status: FileStatusKind::LeftOnly,
                ref_badge: None,
            },
            FileStatus {
                path: ".env".into(),
                status: FileStatusKind::Modified,
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
            unreadable: 0,
        },
        errors: Vec::new(),
    }
}

#[test]
fn test_format_status_text() {
    let text = format_status_text(&sample_status(), false);
    assert!(text.contains("M src/config.ts"));
    assert!(text.contains("L src/new.ts"));
    assert!(text.lines().any(|line| line == "M .env"), "{text}");
    assert!(text.contains("Summary: 2 modified, 1 left only"));
}

#[test]
fn test_format_status_text_summary_only() {
    let text = format_status_text(&sample_status(), true);
    assert!(!text.contains("src/config.ts"));
    assert!(text.contains("Summary:"));
}

#[test]
fn test_format_status_text_agent_connected() {
    let mut output = sample_status();
    output.agent = Some(AgentStatus::Connected);
    let text = format_status_text(&output, false);
    assert!(text.contains("Agent: connected"));
}

#[test]
fn test_format_status_text_no_agent() {
    let output = sample_status();
    let text = format_status_text(&output, false);
    assert!(!text.contains("Agent:"));
}

#[test]
fn test_status_json_agent_field() {
    let mut output = sample_status();
    output.agent = Some(AgentStatus::Connected);
    let json = format_json(&output).unwrap();
    assert!(json.contains("\"agent\""));
    assert!(json.contains("\"connected\""));
}

#[test]
fn test_status_json_no_agent_field_when_none() {
    let output = sample_status();
    let json = format_json(&output).unwrap();
    assert!(!json.contains("\"agent\""));
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
        binary: false,
        symlink: false,
        link_targets: None,
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
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
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
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: true,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
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
            hunk_info: None,
        }],
        skipped: vec![MergeSkipped {
            path: "kind.txt".into(),
            reason: "source and destination have different file types".into(),
        }],
        deleted: vec![],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Merged: a.rs (backup: a.rs.bak)"));
    assert!(text.contains("Skipped: kind.txt (source and destination have different file types)"));
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
        agent: None,
        files: Some(vec![
            FileStatus {
                path: "a.rs".into(),
                status: FileStatusKind::Modified,
                ref_badge: Some("differs".into()),
            },
            FileStatus {
                path: "b.rs".into(),
                status: FileStatusKind::Equal,
                ref_badge: Some("missing_in_ref".into()),
            },
            FileStatus {
                path: "c.rs".into(),
                status: FileStatusKind::Modified,
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
            unreadable: 0,
        },
        errors: Vec::new(),
    };
    let text = format_status_text(&output, false);
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
        agent: None,
        files: Some(vec![FileStatus {
            path: "a.rs".into(),
            status: FileStatusKind::Modified,
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
            unreadable: 0,
        },
        errors: Vec::new(),
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
        binary: false,
        symlink: false,
        link_targets: None,
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
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
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
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(!text.contains("ref"));
}

#[test]
fn test_format_diff_text_binary() {
    // T-3: binary=true の場合 "Binary files differ" が出力される
    let output = DiffOutput {
        path: "image.png".into(),
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        binary: true,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Binary files differ"));
    assert!(text.contains("--- a/image.png"));
    assert!(text.contains("+++ b/image.png"));
    // hunks の @@ ヘッダは出力されない
    assert!(!text.contains("@@"));
}

#[test]
fn test_format_diff_text_symlink() {
    let output = DiffOutput {
        path: "link".into(),
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        binary: false,
        symlink: true,
        link_targets: Some(crate::service::types::LinkTargets {
            left: Some("before.txt".into()),
            right: Some("after.txt".into()),
            ref_: None,
        }),
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Symbolic link targets differ"));
    assert!(!text.contains("Binary files differ"));
}

#[test]
fn test_format_merge_text_dry_run_prefix() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "a.rs".into(),
            status: "would merge".into(),
            backup: None,
            ref_badge: None,
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![],
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
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![],
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
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        ref_: Some(SourceInfo {
            label: "staging".into(),
            root: "/s".into(),
        }),
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Merged: a.rs [ref≠]"));
}

#[test]
fn test_format_merge_text_hunk_merge_info() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "src/foo.rs".into(),
            status: "merged".into(),
            hunk_info: Some(HunkMergeInfo {
                hunks_applied: vec![0, 2, 5],
                hunks_total: 8,
                direction: "left_to_right".into(),
            }),
            ..Default::default()
        }],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(
        text.contains("(hunks: 0,2,5/8)"),
        "hunk info missing in: {}",
        text
    );
    assert!(text.contains("Merged: src/foo.rs"));
}

#[test]
fn test_format_merge_text_no_hunk_info_for_normal_merge() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "bar.rs".into(),
            status: "ok".into(),
            backup: None,
            ref_badge: None,
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(
        !text.contains("hunks:"),
        "hunk info should not appear in normal merge: {}",
        text
    );
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
        binary: false,
        symlink: false,
        link_targets: None,
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
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    }
}

#[test]
fn test_format_multi_diff_text_single_file() {
    let output = MultiDiffOutput {
        files: vec![sample_diff("a.rs")],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 1,
            files_with_changes: 1,
        },
        truncated: false,
        changed_files_total: None,
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
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 5,
            files_with_changes: 2,
        },
        truncated: false,
        changed_files_total: None,
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
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 10,
            files_with_changes: 1,
        },
        truncated: true,
        changed_files_total: Some(10),
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
        agent: None,
        files: Some(vec![]),
        summary: StatusSummary::default(),
        errors: Vec::new(),
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

// ── binary diff with SHA-256 hashes ──

#[test]
fn test_format_diff_text_binary_with_hashes() {
    let output = DiffOutput {
        path: "image.png".into(),
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        binary: true,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: Some("abc123def456".into()),
        right_hash: Some("789xyz000111".into()),
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(
        text.contains(
            "Binary files differ (left: sha256=abc123def456, right: sha256=789xyz000111)"
        )
    );
    assert!(!text.contains("@@"));
}

#[test]
fn test_format_diff_text_binary_left_only_hash() {
    let output = DiffOutput {
        path: "image.png".into(),
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        binary: true,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: Some("abc123".into()),
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Binary files differ (left: sha256=abc123, right: missing)"));
}

#[test]
fn test_format_diff_text_binary_no_hashes_fallback() {
    let output = DiffOutput {
        path: "image.png".into(),
        left: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "dev".into(),
            root: "/r".into(),
        },
        ref_: None,
        binary: true,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Binary files differ"));
    assert!(!text.contains("sha256="));
}

#[test]
fn test_diff_output_binary_hash_serialization() {
    let output = DiffOutput {
        path: "image.png".into(),
        left: SourceInfo {
            label: "l".into(),
            root: ".".into(),
        },
        right: SourceInfo {
            label: "r".into(),
            root: "/r".into(),
        },
        ref_: None,
        binary: true,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: Some("aaa".into()),
        right_hash: Some("bbb".into()),
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let json = format_json(&output).unwrap();
    assert!(json.contains("\"left_hash\""));
    assert!(json.contains("\"aaa\""));
    assert!(json.contains("\"right_hash\""));
    assert!(json.contains("\"bbb\""));
}

#[test]
fn test_diff_output_text_no_hash_in_json() {
    // テキストファイルの diff では left_hash/right_hash は None → JSON に含まれない
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
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let json = format_json(&output).unwrap();
    assert!(!json.contains("left_hash"));
    assert!(!json.contains("right_hash"));
}

// ── MergeOutcome formatter tests ──

#[test]
fn test_format_merge_outcome_text_no_files() {
    let text = format_merge_outcome_text(&MergeOutcome::NoFilesToMerge);
    assert_eq!(text, "no files to merge in the specified path(s)");
}

#[test]
fn test_format_merge_outcome_text_r2r_blocked() {
    let text = format_merge_outcome_text(&MergeOutcome::R2rBlocked {
        left: "develop".into(),
        right: "staging".into(),
    });
    assert!(text.contains("Warning: merging between two remote servers"));
    assert!(text.contains("develop → staging"));
    assert!(text.contains("--force"));
}

#[test]
fn test_format_merge_outcome_json_no_files() {
    let json = format_merge_outcome_json(&MergeOutcome::NoFilesToMerge).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["merged"], serde_json::json!([]));
    assert_eq!(v["skipped"], serde_json::json!([]));
    assert_eq!(v["failed"], serde_json::json!([]));
}

#[test]
fn test_format_merge_outcome_json_r2r_blocked() {
    let json = format_merge_outcome_json(&MergeOutcome::R2rBlocked {
        left: "develop".into(),
        right: "staging".into(),
    })
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["merged"], serde_json::json!([]));
    assert_eq!(v["failed"][0]["path"], "");
    assert!(
        v["failed"][0]["error"]
            .as_str()
            .unwrap()
            .contains("--force")
    );
}

#[test]
fn test_format_merge_outcome_json_r2r_blocked_no_root_path() {
    let json = format_merge_outcome_json(&MergeOutcome::R2rBlocked {
        left: "develop".into(),
        right: "staging".into(),
    })
    .unwrap();
    // root パスが含まれないことを検証
    assert!(!json.contains("/var/www"));
    assert!(!json.contains("/home/"));
}

// ── conflict output tests ──

#[test]
fn test_format_diff_text_with_conflicts() {
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
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 2,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(text.contains("Conflicts: 2 region(s)"));
}

#[test]
fn test_format_diff_text_no_conflicts() {
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
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let text = format_diff_text(&output);
    assert!(!text.contains("Conflicts"));
}

#[test]
fn test_format_multi_diff_text_with_conflicts() {
    let mut diff = sample_diff("a.rs");
    diff.conflict_count = 3;
    let output = MultiDiffOutput {
        files: vec![diff],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 1,
            files_with_changes: 1,
        },
        truncated: false,
        changed_files_total: None,
    };
    let text = format_multi_diff_text(&output);
    assert!(text.contains("3 conflict(s) detected"));
}

#[test]
fn test_format_multi_diff_text_no_conflicts() {
    let output = MultiDiffOutput {
        files: vec![sample_diff("a.rs")],
        errors: vec![],
        summary: MultiDiffSummary {
            scanned_files: 1,
            files_with_changes: 1,
        },
        truncated: false,
        changed_files_total: None,
    };
    let text = format_multi_diff_text(&output);
    assert!(!text.contains("conflict(s) detected"));
}

#[test]
fn test_diff_output_conflict_count_zero_omitted_in_json() {
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
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        conflict_count: 0,
        conflict_regions: vec![],
        encodings: Default::default(),
        byte_only_difference: false,
    };
    let json = format_json(&output).unwrap();
    assert!(!json.contains("conflict_count"));
    assert!(!json.contains("conflict_regions"));
}

// ── backup list / rollback text tests ──

#[test]
fn test_format_backup_list_text_with_sessions() {
    let output = BackupListOutput {
        target: SourceInfo {
            label: "develop".into(),
            root: "/var/www".into(),
        },
        sessions: vec![
            BackupSession::new(
                "20240115-140000".into(),
                vec![
                    BackupEntry {
                        path: "src/config.ts".into(),
                        size: Some(1234),
                        link_target: None,
                    },
                    BackupEntry {
                        path: "src/index.ts".into(),
                        size: Some(5678),
                        link_target: None,
                    },
                ],
                false,
            ),
            BackupSession::new(
                "20240114-100000".into(),
                vec![BackupEntry {
                    path: "src/old.ts".into(),
                    size: Some(456),
                    link_target: None,
                }],
                true,
            ),
        ],
    };
    let text = format_backup_list_text(&output);
    assert!(text.contains("Backup sessions for develop:"));
    assert!(text.contains("20240115-140000 (2 files)"));
    assert!(text.contains("src/config.ts (1234 bytes)"));
    assert!(text.contains("20240114-100000 (1 file) [expired]"));
}

#[test]
fn test_format_backup_list_text_empty() {
    let output = BackupListOutput {
        target: SourceInfo {
            label: "local".into(),
            root: ".".into(),
        },
        sessions: vec![],
    };
    let text = format_backup_list_text(&output);
    assert!(text.contains("no backup sessions found"));
}

#[test]
fn test_format_rollback_text_success() {
    let output = RollbackOutput {
        target: SourceInfo {
            label: "develop".into(),
            root: "/var/www".into(),
        },
        session_id: "20240115-140000".into(),
        restored: vec![
            RollbackFileResult {
                path: "src/config.ts".into(),
                pre_rollback_backup: Some("20240116-090000".into()),
            },
            RollbackFileResult {
                path: "src/index.ts".into(),
                pre_rollback_backup: Some("20240116-090000".into()),
            },
        ],
        skipped: vec![RollbackSkipped {
            path: "src/app.ts".into(),
            reason: "symlink restore not supported".into(),
        }],
        failed: vec![],
    };
    let text = format_rollback_text(&output);
    assert!(text.contains("Rollback session 20240115-140000 for develop:"));
    assert!(text.contains("\u{2713} src/config.ts (pre-rollback backup: 20240116-090000)"));
    assert!(text.contains("- src/app.ts (skipped: symlink restore not supported)"));
    assert!(text.contains("Restored 2 file(s), skipped 1."));
}

#[test]
fn test_format_rollback_text_with_failures() {
    let output = RollbackOutput {
        target: SourceInfo {
            label: "develop".into(),
            root: "/var/www".into(),
        },
        session_id: "20240115-140000".into(),
        restored: vec![RollbackFileResult {
            path: "a.rs".into(),
            pre_rollback_backup: None,
        }],
        skipped: vec![],
        failed: vec![RollbackFailure {
            path: "b.rs".into(),
            error: "permission denied".into(),
        }],
    };
    let text = format_rollback_text(&output);
    assert!(text.contains("\u{2717} b.rs (error: permission denied)"));
    assert!(text.contains("Restored 1 file(s), failed 1."));
}

#[test]
fn test_diff_output_conflict_count_nonzero_in_json() {
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
        binary: false,
        symlink: false,
        link_targets: None,
        truncated: false,
        hunks: vec![],
        ref_hunks: None,
        left_hash: None,
        right_hash: None,
        ref_hash: None,
        note: None,
        encodings: Default::default(),
        byte_only_difference: false,
        conflict_count: 1,
        conflict_regions: vec![crate::service::types::ConflictArea::Lines(ConflictRegion {
            ref_range: 10..15,
            left_lines: vec!["changed left".into()],
            right_lines: vec!["changed right".into()],
        })],
    };
    let json = format_json(&output).unwrap();
    assert!(json.contains("\"conflict_count\""));
    assert!(json.contains("\"conflict_regions\""));
}

// ── format_merge_text: dry-run / actual backup 表示 ──

#[test]
fn test_format_merge_text_actual_backup_path() {
    // 実行時 + backup あり → "(backup: <path>)" を表示
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "src/main.rs".into(),
            status: "ok".into(),
            backup: Some("/var/backups/main.rs.20260311.bak".into()),
            ref_badge: None,
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Merged: src/main.rs (backup: /var/backups/main.rs.20260311.bak)"));
    assert!(!text.contains("backup enabled"));
}

#[test]
fn test_format_merge_text_dryrun_no_backup() {
    // dry-run + backup なし → バックアップ情報を表示しない
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "src/lib.rs".into(),
            status: "would merge".into(),
            backup: None,
            ref_badge: None,
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Would merge: src/lib.rs"));
    assert!(!text.contains("backup"));
}

// ── format_merge_text: --delete 表示 ──

#[test]
fn test_format_merge_text_with_deleted() {
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "updated.rs".into(),
            status: "ok".into(),
            backup: None,
            ref_badge: None,
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![DeleteFileResult {
            path: "old-file.txt".into(),
            status: DeleteStatus::Ok,
            backup: Some("session123/old-file.txt".into()),
        }],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(
        text.contains("Deleted: old-file.txt (backup: session123/old-file.txt)"),
        "text: {}",
        text
    );
    assert!(text.contains("Merged: updated.rs"));
}

#[test]
fn test_format_merge_text_deleted_no_backup() {
    let output = MergeOutput {
        merged: vec![],
        skipped: vec![],
        deleted: vec![DeleteFileResult {
            path: "remove-me.txt".into(),
            status: DeleteStatus::Ok,
            backup: None,
        }],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(text.contains("Deleted: remove-me.txt"), "text: {}", text);
    assert!(!text.contains("backup"));
}

#[test]
fn test_format_merge_text_no_deleted_field_when_empty() {
    // deleted が空のときは何も表示しない
    let output = MergeOutput {
        merged: vec![MergeFileResult {
            path: "a.rs".into(),
            status: "ok".into(),
            backup: None,
            ref_badge: None,
            hunk_info: None,
        }],
        skipped: vec![],
        deleted: vec![],
        failed: vec![],
        ref_: None,
    };
    let text = format_merge_text(&output);
    assert!(!text.contains("Deleted"));
    assert!(!text.contains("Delete"));
}

// ── format_sync_text ──

#[test]
fn format_sync_text_basic() {
    let output = SyncOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        },
        targets: vec![SyncTargetResult {
            target: SourceInfo {
                label: "server1".into(),
                root: "/app".into(),
            },
            merged: vec![MergeFileResult {
                path: "a.txt".into(),
                status: "ok".into(),
                backup: None,
                ref_badge: None,
                hunk_info: None,
            }],
            skipped: vec![],
            deleted: vec![],
            failed: vec![],
            status: SyncTargetStatus::Success,
        }],
        summary: SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 1,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    };
    let text = format_sync_text(&output);
    assert!(text.contains("Sync: local \u{2192} server1"));
    assert!(text.contains("[server1] success"));
    assert!(text.contains("ok"));
    assert!(text.contains("a.txt"));
    assert!(text.contains("1/1 servers successful"));
}

#[test]
fn format_sync_text_with_delete() {
    let output = SyncOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        },
        targets: vec![SyncTargetResult {
            target: SourceInfo {
                label: "server1".into(),
                root: "/app".into(),
            },
            merged: vec![],
            skipped: vec![],
            deleted: vec![DeleteFileResult {
                path: "old.txt".into(),
                status: DeleteStatus::Ok,
                backup: Some("session123/old.txt".into()),
            }],
            failed: vec![],
            status: SyncTargetStatus::Success,
        }],
        summary: SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 0,
            total_files_deleted: 1,
            total_files_failed: 0,
        },
    };
    let text = format_sync_text(&output);
    assert!(text.contains("deleted"));
    assert!(text.contains("backup: session123/old.txt"));
    assert!(text.contains("1 files deleted"));
}

#[test]
fn format_sync_text_multiple_servers() {
    let output = SyncOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        },
        targets: vec![
            SyncTargetResult {
                target: SourceInfo {
                    label: "server1".into(),
                    root: "/app".into(),
                },
                merged: vec![MergeFileResult {
                    path: "a.txt".into(),
                    status: "ok".into(),
                    backup: None,
                    ref_badge: None,
                    hunk_info: None,
                }],
                skipped: vec![],
                deleted: vec![],
                failed: vec![],
                status: SyncTargetStatus::Success,
            },
            SyncTargetResult {
                target: SourceInfo {
                    label: "server2".into(),
                    root: "/app".into(),
                },
                merged: vec![],
                skipped: vec![],
                deleted: vec![],
                failed: vec![MergeFailure {
                    path: "b.txt".into(),
                    error: "connection lost".into(),
                }],
                status: SyncTargetStatus::Failed,
            },
        ],
        summary: SyncSummary {
            total_servers: 2,
            successful_servers: 1,
            total_files_merged: 1,
            total_files_deleted: 0,
            total_files_failed: 1,
        },
    };
    let text = format_sync_text(&output);
    assert!(text.contains("Sync: local \u{2192} server1, server2"));
    assert!(text.contains("[server1] success"));
    assert!(text.contains("[server2] failed"));
    assert!(text.contains("1/2 servers successful"));
    assert!(text.contains("1 files failed"));
}

#[test]
fn format_sync_text_with_skipped() {
    let output = SyncOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        },
        targets: vec![SyncTargetResult {
            target: SourceInfo {
                label: "server1".into(),
                root: "/app".into(),
            },
            merged: vec![],
            skipped: vec![MergeSkipped {
                path: "kind.txt".into(),
                reason: "source and destination have different file types".into(),
            }],
            deleted: vec![],
            failed: vec![],
            status: SyncTargetStatus::Success,
        }],
        summary: SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 0,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    };
    let text = format_sync_text(&output);
    assert!(text.contains("skip"));
    assert!(text.contains("kind.txt"));
    assert!(text.contains("different file types"));
}

#[test]
fn format_sync_text_dry_run() {
    let output = SyncOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        },
        targets: vec![SyncTargetResult {
            target: SourceInfo {
                label: "server1".into(),
                root: "/app".into(),
            },
            merged: vec![
                MergeFileResult {
                    path: "src/config.ts".into(),
                    status: "would merge".into(),
                    backup: None,
                    ref_badge: None,
                    hunk_info: None,
                },
                MergeFileResult {
                    path: "src/index.ts".into(),
                    status: "would merge".into(),
                    backup: None,
                    ref_badge: None,
                    hunk_info: None,
                },
            ],
            skipped: vec![],
            deleted: vec![],
            failed: vec![],
            status: SyncTargetStatus::Success,
        }],
        summary: SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 2,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    };
    let text = format_sync_text(&output);
    // dry-run ではバッジが "plan" になる
    assert!(text.contains("plan"));
    assert!(text.contains("src/config.ts"));
    assert!(text.contains("src/index.ts"));
}

#[test]
fn format_sync_text_with_backup() {
    let output = SyncOutput {
        left: SourceInfo {
            label: "local".into(),
            root: "/app".into(),
        },
        targets: vec![SyncTargetResult {
            target: SourceInfo {
                label: "server1".into(),
                root: "/app".into(),
            },
            merged: vec![MergeFileResult {
                path: "a.txt".into(),
                status: "ok".into(),
                backup: Some("20260311-120000/a.txt".into()),
                ref_badge: None,
                hunk_info: None,
            }],
            skipped: vec![],
            deleted: vec![],
            failed: vec![],
            status: SyncTargetStatus::Success,
        }],
        summary: SyncSummary {
            total_servers: 1,
            successful_servers: 1,
            total_files_merged: 1,
            total_files_deleted: 0,
            total_files_failed: 0,
        },
    };
    let text = format_sync_text(&output);
    assert!(text.contains("backup: 20260311-120000/a.txt"));
}

// ── Step 1: JsonError serialization ──

#[test]
fn test_json_error_serialize() {
    let err = JsonError {
        error: "something went wrong".to_string(),
    };
    let json = serde_json::to_string(&err).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["error"], "something went wrong");
    // Verify no extra fields
    assert_eq!(parsed.as_object().unwrap().len(), 1);
}

// ── Step 7: L/R symbol regression ──

#[test]
fn test_status_text_left_only_symbol() {
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
        agent: None,
        files: Some(vec![FileStatus {
            path: "only_left.rs".into(),
            status: FileStatusKind::LeftOnly,
            ref_badge: None,
        }]),
        summary: StatusSummary {
            modified: 0,
            left_only: 1,
            right_only: 0,
            equal: 0,
            ref_differs: None,
            ref_only: None,
            ref_missing: None,
            unreadable: 0,
        },
        errors: Vec::new(),
    };
    let text = format_status_text(&output, false);
    assert!(
        text.contains("L only_left.rs"),
        "left_only should use 'L ' prefix"
    );
    assert!(
        !text.contains("+ only_left.rs"),
        "should not use old '+' prefix"
    );
}

#[test]
fn test_status_text_right_only_symbol() {
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
        agent: None,
        files: Some(vec![FileStatus {
            path: "only_right.rs".into(),
            status: FileStatusKind::RightOnly,
            ref_badge: None,
        }]),
        summary: StatusSummary {
            modified: 0,
            left_only: 0,
            right_only: 1,
            equal: 0,
            ref_differs: None,
            ref_only: None,
            ref_missing: None,
            unreadable: 0,
        },
        errors: Vec::new(),
    };
    let text = format_status_text(&output, false);
    assert!(
        text.contains("R only_right.rs"),
        "right_only should use 'R ' prefix"
    );
    assert!(
        !text.contains("- only_right.rs"),
        "should not use old '-' prefix"
    );
}
